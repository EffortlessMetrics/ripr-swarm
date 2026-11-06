//! Boundary trait for per-language fact extraction.
//!
//! See `docs/specs/RIPR-SPEC-0026-language-adapter-contract.md`.

use super::super::{AnalysisOptions, diff::ChangedFile};
use super::rust::PartialDiffScope;
use crate::analysis_outcome::AnalysisLimitation;
use crate::config::OraclePolicy;
use crate::domain::Finding;
use std::path::Path;

/// Persistent per-file fact-cache admission counters for one adapter run
/// (#10725). Real counts only: a `hits` increment means a content-keyed
/// on-disk envelope was decoded and passed its integrity check — never an
/// estimate. `invalidated_files` names the workspace files whose content
/// changed under an entry the previous run persisted (the
/// content-invalidation signal; a stat tuple is never consulted, #2175).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreviewFactCacheStats {
    /// Files served from a persisted envelope that matched on content.
    pub(crate) hits: usize,
    /// Files recomputed because no usable envelope admitted them.
    pub(crate) misses: usize,
    /// Envelopes read but rejected (decode, identity, or integrity failure).
    pub(crate) corrupt_ignored: usize,
    /// Files whose recomputed facts were persisted.
    pub(crate) stores: usize,
    /// Persist attempts that failed (IO or encode).
    pub(crate) store_errors: usize,
    /// Files whose content changed while a prior-run envelope existed.
    pub(crate) invalidated_files: std::collections::BTreeSet<std::path::PathBuf>,
    /// The cache base exists but cannot be used; nothing was read or stored.
    /// Reported once, not as a corrupt entry per file.
    pub(crate) unusable_base: Option<String>,
}

impl PreviewFactCacheStats {
    /// The counters a run that never touched the cache carries. `did_work`
    /// stays false so the `-v` disclosure line stays silent instead of
    /// borrowing numbers the run did not produce.
    pub(crate) fn zero_work() -> Self {
        Self::default()
    }

    pub(crate) fn did_work(&self) -> bool {
        self.hits > 0
            || self.misses > 0
            || self.corrupt_ignored > 0
            || self.stores > 0
            || self.store_errors > 0
            || self.unusable_base.is_some()
    }
}

/// Renders the `-v` persistent fact-cache reuse disclosure for one language
/// (#10725). `None` when the adapter did no cache work — a zero-work run
/// stays silent instead of borrowing counts it did not produce. Re-exported
/// by the typescript `fact_store` (its only producer today).
pub(crate) fn render_fact_cache_line(
    language: &str,
    stats: &PreviewFactCacheStats,
) -> Option<String> {
    if !stats.did_work() {
        return None;
    }
    let mut line = format!(
        "{language} per-file facts: {hits} reused, {misses} recomputed, {stores} stored",
        hits = stats.hits,
        misses = stats.misses,
        stores = stats.stores,
    );
    match stats.corrupt_ignored {
        0 => {}
        1 => line.push_str("; 1 corrupt entry ignored"),
        count => line.push_str(&format!("; {count} corrupt entries ignored")),
    }
    if let Some(reason) = &stats.unusable_base {
        line.push_str(&format!(
            "; cache base unusable, nothing read or stored: {reason}"
        ));
    }
    Some(line)
}

/// Per-language results returned by [`LanguageAdapter::analyze_diff`].
///
/// `findings` are unsorted; the orchestrating pipeline applies the
/// language-neutral sort and summary. `changed_files` is the number of
/// diff entries this adapter handled, used by the summary builder.
/// `partial_scope` is `Some` only when the adapter analyzed a deterministic
/// bounded partition of an over-budget diff (`limited_partial_scope`,
/// RIPR-PROP-0019); the pipeline then restricts the changed-file set handed
/// to every other adapter to the same partition.
#[derive(Clone, Debug, Default)]
pub(crate) struct LanguageDiffResult {
    pub(crate) value_origins: crate::value_origins::ValueOrigins,
    pub(crate) findings: Vec<Finding>,
    /// Test-harness registry projections (#3532): what each exact
    /// registration established for this run. Empty without registrations.
    pub(crate) harness_projections: Vec<crate::analysis::harness_projection::TestHarnessProjection>,
    pub(crate) changed_files: usize,
    /// Number of distinct changed source lines for which the adapter
    /// generated at least one probe. This is a producer fact, not a proxy for
    /// every changed source line.
    pub(crate) candidate_line_count: usize,
    /// Per-output-language breakdown of `changed_files` for adapters that
    /// cover more than one output language — the TypeScript adapter handles
    /// `.ts/.tsx/.mts/.cts` (typescript) and `.js/.jsx/.mjs/.cjs`
    /// (javascript) (#2103 review).
    /// Empty when the adapter covers exactly one language; the pipeline then
    /// attributes `changed_files` to the adapter's own language.
    pub(crate) changed_files_by_language: Vec<(super::LanguageId, usize)>,
    pub(crate) partial_scope: Option<PartialDiffScope>,
    /// Number of accepted-language files intentionally excluded as generated
    /// source. The pipeline records this as a partial run disclosure.
    pub(crate) skipped_files: usize,
    /// Typed adapter-owned limitations that the pipeline publishes in the
    /// shared analysis outcome.
    pub(crate) limitations: Vec<AnalysisLimitation>,
    /// Crate-private numeric diagnostic origins for Rust findings (#4464).
    /// Empty for preview adapters.
    pub(crate) rust_diagnostic_origins: crate::analysis::diagnostic_origin::RustDiagnosticOrigins,
    pub(crate) rust_consumed_sources: crate::analysis::consumed_source::ConsumedRustSources,
    /// Persistent per-file fact-cache admissions for this run (#10725).
    /// `zero_work()` unless the adapter consulted its on-disk store — the
    /// pipeline renders the `-v` reuse line only from real counters.
    pub(crate) preview_fact_cache: PreviewFactCacheStats,
}

/// The stage of a repo run at which a named bound first limited the working
/// set. Stages are ordered: discovery precedes selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RepoLimitingStage {
    /// The discovery walk stopped at its entry cap.
    Discovery,
    /// Discovered files exceeded the working-set limit at selection.
    Selection,
}

/// A bound that actually fired, reported by the producer that owns it. `name`
/// is the producer's own operator-visible bound name (an env override name);
/// it is never derived from a disclosure string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RepoCapFired {
    pub(crate) name: &'static str,
    pub(crate) stage: RepoLimitingStage,
    /// The numeric limit in force when the bound fired.
    pub(crate) limit: usize,
}

/// Typed repo-mode file accounting reported by the producer (#6466). Every
/// field is `None` when the producer did not measure it or only has a lower
/// bound (incomplete discovery); unknown is never zero. When discovered,
/// selected, skipped and omitted are all known:
/// `discovered == selected + skipped + omitted`.
///
/// Meaning is producer-specific:
/// - TypeScript: `selected` is production files up to the working-set limit
///   PLUS every test file (evidence is not capped by that limit); `skipped`
///   is type declaration files only; `omitted` is production files beyond the
///   limit. The walk cap (`RIPR_TS_MAX_WORKSPACE_FILES`) counts candidate
///   TypeScript/JavaScript source files, not every directory entry; a 5x
///   total-entry backstop still ends a walk over a huge non-source tree.
/// - Python: `selected` is production plus evidence files under one combined
///   cap, production taking priority; `skipped` is files excluded by role
///   (vendor, generated, environment); `omitted` is eligible files beyond the
///   cap. There is no walk cap.
///
/// `files.skipped` is distinct from the older `files_skipped` observation
/// (`LanguageRepoResult::skipped_files`): that is a compatibility count whose
/// meaning is producer-specific (TypeScript: capped production files plus
/// diff skips; Python: evidence-stage skips) and feeds the public
/// `LanguageRun` reasons. It is not a role/kind exclusion count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepoFileAccounting {
    /// Adapter-routed files seen. `None` when discovery was incomplete, so no
    /// trustworthy total exists.
    pub(crate) discovered: Option<usize>,
    /// Files taken into the working set (production and evidence).
    pub(crate) selected: Option<usize>,
    /// Files intentionally excluded by role or kind (generated, declaration,
    /// environment), not by a cap.
    pub(crate) skipped: Option<usize>,
    /// Eligible files left out because a cap fired.
    pub(crate) omitted: Option<usize>,
    /// The first bound that limited the run, if any fired.
    pub(crate) first_cap_fired: Option<RepoCapFired>,
}

/// Per-language results returned by [`LanguageAdapter::analyze_repo`].
///
/// `findings` are unsorted. `production_files` is the number of files
/// the adapter classified as production code, used by the summary builder.
#[derive(Clone, Debug, Default)]
pub(crate) struct LanguageRepoResult {
    pub(crate) value_origins: crate::value_origins::ValueOrigins,
    pub(crate) findings: Vec<Finding>,
    /// Test-harness registry projections (#3532); empty without
    /// registrations.
    pub(crate) harness_projections: Vec<crate::analysis::harness_projection::TestHarnessProjection>,
    pub(crate) production_files: usize,
    /// Number of discovered-language files intentionally excluded as generated
    /// source. The pipeline records this as a partial run disclosure.
    pub(crate) skipped_files: usize,
    /// Typed partial-run disclosure (#3554, #2109): `Some` only when the
    /// adapter ran over a capped or otherwise partial repo working set, so
    /// the run can never back a full-denominator claim. The pipeline records
    /// it as a `LanguageRun` with status `Partial` — the same disclosure
    /// channel the Rust repo path uses for generated-file skips — which
    /// human/JSON output renders and gates fail closed on. `None` for
    /// complete runs and honest zeros.
    pub(crate) partial_reason: Option<String>,
    /// Typed file accounting (#6466); unmeasured producers leave it default.
    pub(crate) file_accounting: RepoFileAccounting,
    /// Crate-private numeric diagnostic origins for Rust findings (#4464).
    /// Empty for preview adapters.
    pub(crate) rust_diagnostic_origins: crate::analysis::diagnostic_origin::RustDiagnosticOrigins,
    pub(crate) rust_consumed_sources: crate::analysis::consumed_source::ConsumedRustSources,
}

/// Boundary trait for per-language adapters.
///
/// Pipelines call the adapter for Rust-shaped work (file selection,
/// indexing, probe generation, classification) and then perform the
/// language-neutral sort + summary on the returned `Vec<Finding>`.
///
/// The trait is internal to `crate::analysis`. Per-spec method extensions
/// for fact projection (`extract_facts`, `changed_owners`, ...) land
/// alongside their production consumers when TypeScript and Python
/// preview adapters arrive (see RIPR-SPEC-0027 and RIPR-SPEC-0028).
pub(crate) trait LanguageAdapter {
    /// Returns true when the adapter should handle the given source path.
    fn accepts_path(&self, path: &Path) -> bool;

    /// Produce findings for changed files from a diff.
    ///
    /// The adapter is responsible for filtering `changed_files` to the
    /// subset it accepts, building any language-specific index, applying
    /// oracle policy, generating probes, and classifying them.
    fn analyze_diff(
        &self,
        options: &AnalysisOptions,
        oracle_policy: &OraclePolicy,
        changed_files: &[ChangedFile],
    ) -> Result<LanguageDiffResult, String>;

    /// Produce findings for a whole-repo scan.
    ///
    /// The adapter discovers files it accepts under the workspace root,
    /// classifies production vs. test/example files, builds its index,
    /// generates probes, and classifies them.
    fn analyze_repo(
        &self,
        options: &AnalysisOptions,
        oracle_policy: &OraclePolicy,
    ) -> Result<LanguageRepoResult, String>;
}

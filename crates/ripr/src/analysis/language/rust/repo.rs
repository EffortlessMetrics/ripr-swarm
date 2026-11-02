//! Repository discovery, source-role consumption, and repo-mode analysis for the
//! Rust adapter (RA2 slice for #4786 / #3745).
//!
//! Diff-scoped index construction and the diff pipeline remain in the parent
//! module until RA3.

use super::super::super::probes as analysis_probes;
use super::super::super::{
    AnalysisOptions, classifier, classify, harness_projection, rust_index, workspace,
};
use super::super::LanguageRepoResult;
use super::{
    GeneratedRustSources, LanguageId, RustAdapter, apply_repo_classified_finding, oracles,
    positive_limit_from_env, probe_shape, push_retained_finding,
};
use crate::analysis::diagnostic_origin::{OriginBuildContext, origins_for_rust_findings};
use crate::config::OraclePolicy;
use crate::domain::SourceCurrentness;
use std::collections::{BTreeMap, BTreeSet};

/// Hard analysis-cost guard for the repo-scoped path (#2109).
pub(crate) const REPO_INDEX_FILE_LIMIT: usize = 1200;

/// Env override for [`REPO_INDEX_FILE_LIMIT`].
pub(crate) const REPO_INDEX_FILE_LIMIT_ENV: &str = "RIPR_MAX_REPO_INDEX_FILES";

pub(crate) fn repo_index_file_limit_from_env(
    value: Result<String, std::env::VarError>,
) -> Result<usize, String> {
    positive_limit_from_env(REPO_INDEX_FILE_LIMIT_ENV, REPO_INDEX_FILE_LIMIT, value)
}

/// Fail closed when a repo-scoped working set exceeds the guard (#2109).
pub(crate) fn enforce_repo_index_file_limit(
    file_count: usize,
    scope_limit: usize,
) -> Result<(), String> {
    if file_count <= scope_limit {
        return Ok(());
    }
    Err(format!(
        "repo_scope_oversized: {file_count} indexed Rust files exceed the \
         {REPO_INDEX_FILE_LIMIT_ENV} limit ({scope_limit}); analysis was not run to protect \
         runner memory. Repair route: narrow the scope with a diff-based run (--base/--diff), \
         or raise the limit via {REPO_INDEX_FILE_LIMIT_ENV}=<number>."
    ))
}

impl RustAdapter {
    pub(crate) fn analyze_repo_with_rust_config(
        &self,
        options: &AnalysisOptions,
        oracle_policy: &OraclePolicy,
        rust_config: &crate::config::RustLanguageConfig,
    ) -> Result<LanguageRepoResult, String> {
        let _syntax_memo = crate::analysis::syntax::run_memo::enter();
        let rust_files = workspace::discover_rust_files(&options.root)?;
        let generated_sources = GeneratedRustSources::for_repo(&options.root, rust_config);
        let skipped_files = rust_files
            .iter()
            .filter(|path| generated_sources.contains(path))
            .count();
        let analyzable_rust_files = rust_files
            .iter()
            .filter(|path| !generated_sources.contains(path))
            .cloned()
            .collect::<Vec<_>>();
        let scope_limit = repo_index_file_limit_from_env(std::env::var(REPO_INDEX_FILE_LIMIT_ENV))?;
        enforce_repo_index_file_limit(analyzable_rust_files.len(), scope_limit)?;
        let repo_source_role_context = {
            let mut context = workspace::context_for_files(
                &options.root,
                analyzable_rust_files.iter().map(|path| path.as_path()),
            );
            context.production_like_targets = options.production_like_targets.clone();
            context.harness_targets = rust_index::validated_file_wide_harness_targets(
                &options.root,
                &options.test_harnesses,
            );
            context
        };
        let production_files = analyzable_rust_files
            .iter()
            .filter(|path| {
                workspace::classify_with(path, &repo_source_role_context)
                    .seeds_production_findings()
            })
            .cloned()
            .collect::<Vec<_>>();

        let mut rust_consumed_sources =
            crate::analysis::consumed_source::ConsumedRustSources::default();
        let loaded_rust_files = analyzable_rust_files
            .iter()
            .map(|file| {
                let full = options.root.join(file);
                let bytes = std::fs::read(&full)
                    .map_err(|err| format!("failed to read {}: {err}", full.display()))?;
                rust_consumed_sources.record(file, Some(&bytes));
                Ok((file.clone(), bytes))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let cached = rust_index::build_analysis_index_from_loaded_files(
            &options.root,
            &loaded_rust_files,
            &options.test_harnesses,
        )?;
        #[cfg(test)]
        {
            rust_consumed_sources.file_fact_cache = cached.file_fact_cache.clone();
        }
        let mut index = cached.index;
        if let Some(disclosure) = rust_index::lexical_fallback_disclosure(&index) {
            eprintln!("{disclosure}");
        }
        if let Some(disclosure) = rust_index::include_resolution_disclosure(&index) {
            eprintln!("{disclosure}");
        }
        if let Some(disclosure) = rust_index::module_composition_disclosure(&index) {
            eprintln!("{disclosure}");
        }
        rust_index::apply_oracle_policy(&mut index, oracle_policy);
        let mut related_test_candidate_index = None;
        let property_macro_mentions =
            oracles::PropertyMacroMentionIndex::new(&index, &options.root);
        let transitive_reach = classify::TransitiveReachIndex::new(&index);

        let mut findings = Vec::new();
        let value_origins = std::sync::Mutex::new(crate::value_origins::ValueOrigins::default());
        let mut parser_spans = BTreeMap::new();

        let manifest_dir_prefixes =
            crate::analysis::seam_cache::workspace_manifest_dir_prefixes(&options.root);
        let dependency_adjacency = workspace::PathDependencyAdjacency::build(
            &crate::analysis::seam_cache::workspace_graph_provenance(&options.root),
        );
        let unindexed_manifests = BTreeSet::new();
        let dependency_edges = classify::DependencyEdgeContext {
            adjacency: &dependency_adjacency,
            manifest_dir_prefixes: &manifest_dir_prefixes,
            index: &index,
            unindexed_manifests: &unindexed_manifests,
            root: &options.root,
        };

        for path in &production_files {
            let probes = analysis_probes::probes_for_repo_file_seeded(&options.root, path, &index);
            for seeded in probes {
                seeded.record_span(&mut parser_spans);
                let probe = seeded.probe;
                let related_test_candidate_index =
                    related_test_candidate_index.get_or_insert_with(|| {
                        classify::RelatedTestCandidateIndex::new(&index)
                            .with_analysis_root(&options.root)
                    });
                let (mut finding, pre_cap_syntax_only) =
                    classifier::classify_probe_with_candidate_index_and_origins_with_direct_rows(
                        &probe,
                        &index,
                        true,
                        Some(&dependency_edges),
                        related_test_candidate_index,
                        &transitive_reach,
                        None,
                        Some(&value_origins),
                    );
                finding.language = Some(LanguageId::Rust);
                finding.source_currentness = SourceCurrentness::CandidateCurrent;
                apply_repo_classified_finding(
                    &mut finding,
                    &probe,
                    &index,
                    &property_macro_mentions,
                    &transitive_reach,
                    &dependency_edges,
                    path,
                    pre_cap_syntax_only,
                );
                if finding.static_limit_kind.is_none() {
                    finding.canonical_gap = probe_shape::canonical_rust_gap_for(
                        path,
                        probe.owner.as_ref(),
                        &probe.family,
                        &probe.expression,
                    );
                }
                push_retained_finding(&mut findings, finding);
            }
        }

        let rust_diagnostic_origins = origins_for_rust_findings(
            &findings,
            &OriginBuildContext {
                root: &options.root,
                loaded_files: &loaded_rust_files,
                index: &index,
                parser_spans: &parser_spans,
            },
        );

        Ok(LanguageRepoResult {
            value_origins: value_origins.into_inner().unwrap_or_default(),
            findings,
            harness_projections: harness_projection::projections_from_index(
                &index,
                &options.test_harnesses,
            ),
            production_files: production_files.len(),
            skipped_files,
            partial_reason: None,
            file_accounting: Default::default(),
            rust_diagnostic_origins,
            rust_consumed_sources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        REPO_INDEX_FILE_LIMIT, REPO_INDEX_FILE_LIMIT_ENV, enforce_repo_index_file_limit,
        repo_index_file_limit_from_env,
    };

    /// RA2 ownership pin: repo scope guard errors stay owned by `repo.rs`.
    #[test]
    fn repo_scope_guard_error_prefix_is_stable() -> Result<(), String> {
        let err = match enforce_repo_index_file_limit(801, 800) {
            Ok(()) => return Err("expected guard failure".to_string()),
            Err(error) => error,
        };
        assert!(
            err.starts_with("repo_scope_oversized:"),
            "unexpected guard error: {err}"
        );
        assert!(
            err.contains(REPO_INDEX_FILE_LIMIT_ENV),
            "guard must name the env override: {err}"
        );
        Ok(())
    }

    #[test]
    fn repo_index_file_limit_env_parsing() -> Result<(), String> {
        assert_eq!(REPO_INDEX_FILE_LIMIT, 1200);
        let unset = repo_index_file_limit_from_env(Err(std::env::VarError::NotPresent))
            .map_err(|error| error.to_string())?;
        assert_eq!(
            unset, REPO_INDEX_FILE_LIMIT,
            "default must be the {REPO_INDEX_FILE_LIMIT_ENV} guard"
        );
        let raised = repo_index_file_limit_from_env(Ok("5000".to_string()))
            .map_err(|error| error.to_string())?;
        assert_eq!(raised, 5000);
        for bad in ["", "  ", "lots", "1.5", "0", "-5"] {
            if repo_index_file_limit_from_env(Ok(bad.to_string())).is_ok() {
                return Err(format!("invalid override {bad:?} must fail closed"));
            }
        }
        Ok(())
    }

    #[test]
    fn enforce_repo_index_file_limit_is_fail_closed_exactly_over_the_guard() -> Result<(), String> {
        enforce_repo_index_file_limit(800, 800).map_err(|error| error.to_string())?;
        let err = match enforce_repo_index_file_limit(801, 800) {
            Err(err) => err,
            Ok(()) => return Err("one over the limit must fail".to_string()),
        };
        for needle in [
            "repo_scope_oversized",
            "801 indexed Rust files",
            "RIPR_MAX_REPO_INDEX_FILES",
            "--base/--diff",
        ] {
            assert!(err.contains(needle), "error missing `{needle}`: {err}");
        }
        Ok(())
    }
}

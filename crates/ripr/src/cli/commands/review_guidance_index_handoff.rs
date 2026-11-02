//! Prepared-workspace handoff resolution for `review-comments` (#4692).
//!
//! Today only validates the future `analysis_index_handoff` envelope on
//! `--check-output` and returns `None` so guidance keeps the documented
//! compatibility recompute route. Loading a prepared workspace from disk or
//! arena bytes is a follow-up slice.

use crate::analysis::PreparedDiffScopedClassifiedWorkspace;
use crate::config::RiprConfig;
use std::path::Path;

pub(crate) enum ReviewGuidancePreparedWorkspace<'a> {
    Recompute {
        /// Human-readable reason the compatibility route was taken.
        reason: &'static str,
    },
    Reuse {
        prepared: &'a PreparedDiffScopedClassifiedWorkspace,
    },
}

/// Resolve whether guidance may reuse a prepared workspace for this dispatch.
pub(crate) fn resolve_review_guidance_prepared_workspace<'a>(
    check_output: Option<&Path>,
    _root: &Path,
    _base: &str,
    _head: &str,
    _config: &RiprConfig,
    in_memory: Option<&'a PreparedDiffScopedClassifiedWorkspace>,
) -> Result<ReviewGuidancePreparedWorkspace<'a>, String> {
    if let Some(prepared) = in_memory {
        return Ok(ReviewGuidancePreparedWorkspace::Reuse { prepared });
    }
    let Some(path) = check_output else {
        return Ok(ReviewGuidancePreparedWorkspace::Recompute {
            reason: "no_check_output_handoff_requested",
        });
    };
    let text = crate::bounded_input::read_to_string(path).map_err(|err| {
        format!(
            "review-comments --check-output {} is invalid: read failed: {err}",
            crate::output::pr_inline_comment_publish_plan::display_path(path)
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
        format!(
            "review-comments --check-output {} is invalid: JSON parse failed: {err}",
            crate::output::pr_inline_comment_publish_plan::display_path(path)
        )
    })?;
    if value.get("analysis_index_handoff").is_some() {
        return Err(format!(
            "review-comments --check-output {} carries analysis_index_handoff but this ripr build cannot load it yet; regenerate check output without the field or upgrade ripr",
            crate::output::pr_inline_comment_publish_plan::display_path(path)
        ));
    }
    Ok(ReviewGuidancePreparedWorkspace::Recompute {
        reason: "missing_analysis_index_handoff",
    })
}

pub(crate) fn took_index_recompute_compatibility_route(
    decision: &ReviewGuidancePreparedWorkspace<'_>,
    check_output: Option<&Path>,
) -> bool {
    matches!(
        (decision, check_output),
        (ReviewGuidancePreparedWorkspace::Recompute { .. }, Some(_))
    )
}

/// Whether the completed receipt should disclose index recompute (#4692).
///
/// The compatibility route can still take the no-impact fast path without
/// building an index; [`ScopedClassifiedSeamInventory::consumed_sources`] stays
/// empty in that case.
pub(crate) fn should_record_index_recompute_compatibility(
    took_compat_route: bool,
    inventory: &crate::analysis::ScopedClassifiedSeamInventory,
) -> bool {
    took_compat_route && !inventory.consumed_sources.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RiprConfig;

    #[test]
    fn missing_handoff_field_selects_recompute_with_check_output() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-handoff-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let check_path = dir.join("check.json");
        std::fs::write(&check_path, br#"{"schema_version":"0.1","tool":"ripr"}"#)
            .map_err(|err| err.to_string())?;
        let decision = resolve_review_guidance_prepared_workspace(
            Some(check_path.as_path()),
            dir.as_path(),
            "base",
            "HEAD",
            &RiprConfig::default(),
            None,
        )?;
        assert!(matches!(
            decision,
            ReviewGuidancePreparedWorkspace::Recompute {
                reason: "missing_analysis_index_handoff"
            }
        ));
        assert!(took_index_recompute_compatibility_route(
            &decision,
            Some(check_path.as_path())
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn recompute_disclosure_requires_observed_rust_sources() {
        use crate::analysis::ScopedClassifiedSeamInventory;
        use crate::analysis::seam_cache::{FileFactCacheStats, RepoSeamCacheKey};

        let fast_path_inventory = ScopedClassifiedSeamInventory {
            classified: Vec::new(),
            classified_seams_considered: 0,
            file_fact_cache: FileFactCacheStats::zero_work(),
            workspace_cache_key: RepoSeamCacheKey {
                schema_version: String::new(),
                analyzer_version: String::new(),
                workspace_root_hash: String::new(),
                files_content_hash: String::new(),
                cfg_features_hash: String::new(),
                config_hash: String::new(),
                test_intent_hash: String::new(),
                suppressions_hash: String::new(),
                workspace_manifests_hash: String::new(),
                lockfile_hash: String::new(),
                toolchain_hash: String::new(),
                seam_limit_key: String::new(),
            },
            total_rust_files: 0,
            total_production_files: 0,
            scoped_production_files: Vec::new(),
            changed_production_files: Vec::new(),
            parser_declined_files: Vec::new(),
            immediate_caller_files: Vec::new(),
            unevaluated_seams: 0,
            absent_changed_files: Vec::new(),
            repo_seam_limit: None,
            consumed_sources: Default::default(),
        };
        assert!(
            !should_record_index_recompute_compatibility(true, &fast_path_inventory),
            "no-impact-style inventory must not trigger rebuild disclosure"
        );
    }
}

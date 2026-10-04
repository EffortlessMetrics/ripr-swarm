//! Binds the inline test-module region cage to one repair attempt (#5210).
//!
//! A repair whose selected target is a production Rust file may edit only the
//! body of that file's one governed inline `#[cfg(test)]` module. The before
//! phase captures the exact source bytes and the module identity; the after
//! phase re-derives the region authority from those retained bytes and runs
//! the producer-neutral validator against the current file. Every copy a
//! commit could carry (worktree, index, and any commit on top of the prepared
//! head) must be the validated bytes or unchanged. Nothing here decides
//! whether the added test is useful; that stays with the after-phase analysis.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    InlineTestRegionError, InlineTestRegionRejectReason, InlineTestRegionStatus,
    authority_from_source, nearest_package_identity, read_contained_regular_file,
    validate::validate_inline_test_region_edit,
};

/// Before-phase capture of the selected target's one governed inline test
/// module. The retained `source` is the exact before text the after phase
/// validates against; a digest alone could not show which bytes moved.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct InlineTestRegionBaseline {
    pub(crate) path: String,
    pub(crate) module_name: String,
    pub(crate) package_identity: String,
    pub(crate) source: String,
}

/// After-phase observation of the selected target against its captured
/// inline region. `admitted` is true only when the validator admitted the
/// worktree bytes and every index or committed copy is those bytes or
/// unchanged; `reason` names the first refusal otherwise.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct InlineTestRegionObservation {
    pub(crate) path: String,
    pub(crate) module: String,
    pub(crate) admitted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

/// The copies of the selected target that a commit could carry, read from
/// Git by the caller, which owns repository observation.
pub(crate) struct InlineTargetCopies<'a> {
    /// Current worktree bytes of the selected target, when readable.
    pub(crate) worktree: Result<String, String>,
    /// `true` when the index copy is unchanged since the before phase or is
    /// the worktree bytes.
    pub(crate) index_matches: bool,
    /// `true` when HEAD did not move, the committed copy is unchanged, or the
    /// committed copy is the worktree bytes.
    pub(crate) head_matches: bool,
    pub(crate) baseline: &'a InlineTestRegionBaseline,
}

/// Capture the one governed inline cfg-test module of `relative_file` under
/// the canonical repository `root`. Refuses, with the next step named, when
/// the file has no inline governed test module, more than one candidate, an
/// out-of-line `mod tests;`, or is a test surface, generated, unreadable,
/// or unparseable.
pub(crate) fn capture_attempt_inline_region(
    root: &Path,
    relative_file: &str,
) -> Result<InlineTestRegionBaseline, String> {
    let source = read_contained_regular_file(root, relative_file)
        .map_err(|error| refusal(relative_file, &describe_error(&error)))?;
    let module_name =
        unique_governed_inline_module(&source).map_err(|reason| refusal(relative_file, reason))?;
    let package_identity = nearest_package_identity(root, relative_file);
    authority_from_source(
        relative_file,
        &source,
        &module_name,
        package_identity.clone(),
        &[],
    )
    .map_err(|error| refusal(relative_file, &describe_error(&error)))?;
    Ok(InlineTestRegionBaseline {
        path: relative_file.to_string(),
        module_name,
        package_identity,
        source,
    })
}

/// Validate the selected target's current copies against the captured region.
pub(crate) fn observe_attempt_inline_region(
    copies: InlineTargetCopies<'_>,
) -> InlineTestRegionObservation {
    let baseline = copies.baseline;
    let observation = |admitted: bool, reason: Option<String>| InlineTestRegionObservation {
        path: baseline.path.clone(),
        module: baseline.module_name.clone(),
        admitted,
        reason,
    };
    let after = match copies.worktree {
        Ok(after) => after,
        Err(reason) => return observation(false, Some(format!("target_unreadable: {reason}"))),
    };
    let authority = match authority_from_source(
        &baseline.path,
        &baseline.source,
        &baseline.module_name,
        baseline.package_identity.clone(),
        &[],
    ) {
        Ok(authority) => authority,
        Err(error) => return observation(false, Some(reason_label(error.reason()))),
    };
    let verdict = validate_inline_test_region_edit(&baseline.source, &authority, &after);
    match verdict.status {
        InlineTestRegionStatus::Admitted => {}
        InlineTestRegionStatus::NotARepair => {
            return observation(false, Some("no_test_function_added".to_string()));
        }
        InlineTestRegionStatus::Rejected => {
            return observation(false, Some(reason_label(verdict.reason)));
        }
    }
    if !copies.index_matches {
        return observation(false, Some("index_copy_not_validated".to_string()));
    }
    if !copies.head_matches {
        return observation(false, Some("committed_copy_not_validated".to_string()));
    }
    observation(true, None)
}

/// The module name of the file's one governed inline cfg-test module, by the
/// same uniqueness law the InlineUnit producer applies: exactly one inline
/// module and no out-of-line `mod tests;` that could compete.
fn unique_governed_inline_module(source: &str) -> Result<String, &'static str> {
    let modules = crate::analysis::governed_cfg_test_modules(source).ok_or(
        "the file does not parse cleanly, so its test-module region cannot be established",
    )?;
    let inline = modules
        .iter()
        .filter(|module| module.is_inline)
        .collect::<Vec<_>>();
    let out_of_line = modules.iter().any(|module| !module.is_inline);
    match (inline.as_slice(), out_of_line) {
        ([module], false) => Ok(module.name.clone()),
        ([], true) => Err("its only test module is out-of-line (`mod tests;`)"),
        ([], false) => Err("it has no inline `#[cfg(test)]` module"),
        _ => Err("it has more than one candidate test module"),
    }
}

fn refusal(relative_file: &str, reason: &str) -> String {
    format!(
        "selected edit target `{relative_file}` is a production file; a repair may edit it only inside exactly one existing inline `#[cfg(test)]` module, but {reason}. No attempt was created. Add the test in a `tests/` file instead, or give the file one inline `#[cfg(test)] mod tests {{ }}` and rerun the before phase"
    )
}

fn describe_error(error: &InlineTestRegionError) -> String {
    match error {
        InlineTestRegionError::Path(message) => format!("it could not be read ({message})"),
        InlineTestRegionError::Unsupported { reason } => {
            format!(
                "its region is not admitted (`{}`)",
                reason_label(Some(*reason))
            )
        }
    }
}

fn reason_label(reason: Option<InlineTestRegionRejectReason>) -> String {
    let Some(reason) = reason else {
        return "unclassified_refusal".to_string();
    };
    match reason {
        InlineTestRegionRejectReason::StaleSourceDigest => "stale_source_digest",
        InlineTestRegionRejectReason::StaleModuleAnchor => "stale_module_anchor",
        InlineTestRegionRejectReason::ProductionEdit => "production_edit",
        InlineTestRegionRejectReason::ModuleDeclarationChanged => "module_declaration_changed",
        InlineTestRegionRejectReason::CfgBasisChanged => "cfg_basis_changed",
        InlineTestRegionRejectReason::WrongModule => "wrong_module",
        InlineTestRegionRejectReason::UnsupportedModuleKind => "unsupported_module_kind",
        InlineTestRegionRejectReason::Unparseable => "unparseable",
        InlineTestRegionRejectReason::MissingRegion => "missing_region",
        InlineTestRegionRejectReason::AmbiguousRegion => "ambiguous_region",
        InlineTestRegionRejectReason::PathEscape => "path_escape",
        InlineTestRegionRejectReason::WrongRoot => "wrong_root",
        InlineTestRegionRejectReason::NonTestSubject => "non_test_subject",
        InlineTestRegionRejectReason::ExistingAuthorityRewritten => "existing_authority_rewritten",
        InlineTestRegionRejectReason::NotPureInsertion => "not_pure_insertion",
    }
    .to_string()
}

/// Read the selected target's worktree bytes under the same containment rules
/// the capture used (no symlink, no escape, bounded size).
pub(crate) fn read_attempt_inline_target(
    root: &Path,
    relative_file: &str,
) -> Result<String, String> {
    read_contained_regular_file(root, relative_file).map_err(|error| describe_error(&error))
}

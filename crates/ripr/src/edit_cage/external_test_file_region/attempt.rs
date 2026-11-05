//! Binds the external Rust test-file pure-insertion cage to one repair attempt.

use std::ffi::OsStr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::edit_cage::inline_test_region::{
    InlineTestRegionRejectReason, InlineTestRegionStatus, read_attempt_inline_target,
    source_file_body, validate_external_test_file_edit,
};

/// Before-phase capture of the selected external Rust test file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ExternalTestFileBaseline {
    pub(crate) path: String,
    pub(crate) source: String,
}

/// After-phase observation against the captured before text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ExternalTestFileObservation {
    pub(crate) path: String,
    pub(crate) admitted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

pub(crate) struct ExternalTargetCopies<'a> {
    pub(crate) worktree: Result<String, String>,
    pub(crate) index_matches: bool,
    pub(crate) head_matches: bool,
    pub(crate) object_unreadable: Option<String>,
    pub(crate) baseline: &'a ExternalTestFileBaseline,
}

pub(crate) fn capture_attempt_external_region(
    root: &Path,
    relative_file: &str,
    absent_from_baseline: bool,
) -> Result<ExternalTestFileBaseline, String> {
    if !crate::analysis::is_test_surface_path(relative_file) {
        return Err(refusal(relative_file, "it is not a test-surface path"));
    }
    if Path::new(relative_file).extension() != Some(OsStr::new("rs")) {
        return Err(refusal(
            relative_file,
            "pure-insertion confinement applies only to Rust test files",
        ));
    }
    let source = if absent_from_baseline {
        // The repository snapshot owner established clean absence in the
        // worktree, index and recorded HEAD, including safe path components.
        String::new()
    } else {
        read_attempt_inline_target(root, relative_file)
            .map_err(|error| refusal(relative_file, &format!("it could not be read ({error})")))?
    };
    let generated_file_patterns = crate::config::load_for_root(root)
        .map_err(|error| {
            refusal(
                relative_file,
                &format!("its repository configuration could not be read ({error})"),
            )
        })?
        .languages
        .rust
        .generated_file_patterns;
    if crate::analysis::is_generated_rust_file_with_patterns(
        Path::new(relative_file),
        &generated_file_patterns,
    ) {
        return Err(refusal(
            relative_file,
            "it is a generated file (built-in naming or `languages.rust.generated_file_patterns`)",
        ));
    }
    if source_file_body(&source).is_err() {
        return Err(refusal(
            relative_file,
            "it does not parse cleanly, so its test items cannot be established",
        ));
    }
    Ok(ExternalTestFileBaseline {
        path: relative_file.to_string(),
        source,
    })
}

pub(crate) fn observe_attempt_external_region(
    copies: ExternalTargetCopies<'_>,
) -> ExternalTestFileObservation {
    let baseline = copies.baseline;
    let observation = |admitted: bool, reason: Option<String>| ExternalTestFileObservation {
        path: baseline.path.clone(),
        admitted,
        reason,
    };
    let after = match copies.worktree {
        Ok(after) => after,
        Err(reason) => return observation(false, Some(format!("target_unreadable: {reason}"))),
    };
    let verdict = validate_external_test_file_edit(&baseline.source, &after);
    match verdict.status {
        InlineTestRegionStatus::Admitted => {}
        InlineTestRegionStatus::NotARepair => {
            return observation(false, Some("no_test_function_added".to_string()));
        }
        InlineTestRegionStatus::Rejected => {
            return observation(false, Some(reason_label(verdict.reason)));
        }
    }
    if let Some(error) = copies.object_unreadable {
        return observation(false, Some(format!("git_object_unreadable: {error}")));
    }
    if !copies.index_matches {
        return observation(false, Some("index_copy_not_validated".to_string()));
    }
    if !copies.head_matches {
        return observation(false, Some("committed_copy_not_validated".to_string()));
    }
    observation(true, None)
}

/// Refusal when the selected external test file has uncommitted worktree or
/// index changes relative to HEAD.
pub(crate) fn uncommitted_target_refusal(relative_file: &str) -> String {
    format!(
        "selected edit target `{relative_file}` is an external Rust test file with uncommitted changes (worktree or index differs from HEAD); a repair confined to pure insertion starts only from the committed file. No attempt was created. Commit or stash the changes to `{relative_file}` and rerun the before phase"
    )
}

fn refusal(relative_file: &str, reason: &str) -> String {
    format!(
        "selected edit target `{relative_file}` is an external Rust test file; a repair may change it only by pure insertion of new test functions, but {reason}. No attempt was created"
    )
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

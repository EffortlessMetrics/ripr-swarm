use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::SymbolId;

/// Producer-owned disposition for the source subject carried by a diff finding.
///
/// This says which revision, if any, owns an editable candidate source. It is
/// intentionally independent of renderer and gate policy: downstream consumers
/// may project the disposition, but they must not reconstruct it from a line
/// number or from the continued presence of a legacy navigation location.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceCurrentness {
    CandidateCurrent,
    BaseDeleted,
    MovedOrRenamed,
    #[default]
    UnresolvedSubject,
}

/// Revision-specific identity for one source expression.
///
/// The path and range are meaningful only in the revision slot that contains
/// this value (`candidate` or `base`). The normalized expression and optional
/// owner keep a reused line coordinate from being treated as identity by itself.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FindingSourceIdentity {
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub normalized_expression: String,
    pub owner: Option<SymbolId>,
}

impl FindingSourceIdentity {
    pub fn new(
        file: impl Into<PathBuf>,
        start_line: usize,
        end_line: usize,
        normalized_expression: impl Into<String>,
        owner: Option<SymbolId>,
    ) -> Self {
        Self {
            file: file.into(),
            start_line,
            end_line,
            start_column: 1,
            normalized_expression: normalized_expression.into(),
            owner,
        }
    }
}

/// Candidate/base source binding for a finding.
///
/// `candidate_current` always carries `candidate`; `base_deleted` always
/// carries `base` and no candidate edit target. Move and unresolved states may
/// retain either identity when it is known without promoting it to an editable
/// candidate source.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FindingSourceResolution {
    pub currentness: SourceCurrentness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<FindingSourceIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<FindingSourceIdentity>,
}

impl FindingSourceResolution {
    pub fn candidate_current(candidate: FindingSourceIdentity, base: Option<FindingSourceIdentity>) -> Self {
        Self {
            currentness: SourceCurrentness::CandidateCurrent,
            candidate: Some(candidate),
            base,
        }
    }

    pub fn base_deleted(base: FindingSourceIdentity) -> Self {
        Self {
            currentness: SourceCurrentness::BaseDeleted,
            candidate: None,
            base: Some(base),
        }
    }

    pub fn moved_or_renamed(
        candidate: Option<FindingSourceIdentity>,
        base: Option<FindingSourceIdentity>,
    ) -> Self {
        Self {
            currentness: SourceCurrentness::MovedOrRenamed,
            candidate,
            base,
        }
    }

    pub fn unresolved(
        candidate: Option<FindingSourceIdentity>,
        base: Option<FindingSourceIdentity>,
    ) -> Self {
        Self {
            currentness: SourceCurrentness::UnresolvedSubject,
            candidate,
            base,
        }
    }

    pub fn is_empty_unresolved(&self) -> bool {
        self.currentness == SourceCurrentness::UnresolvedSubject
            && self.candidate.is_none()
            && self.base.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_deleted_has_no_candidate_edit_target() {
        let base = FindingSourceIdentity::new("src/lib.rs", 29, 29, "return legacy", None);
        let resolution = FindingSourceResolution::base_deleted(base.clone());

        assert_eq!(resolution.currentness, SourceCurrentness::BaseDeleted);
        assert_eq!(resolution.base, Some(base));
        assert!(resolution.candidate.is_none());
    }

    #[test]
    fn unresolved_default_is_omittable_legacy_state() {
        assert!(FindingSourceResolution::default().is_empty_unresolved());
    }
}

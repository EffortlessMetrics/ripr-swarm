//! Pure edit-cage evaluation for a future durable `RepairAttempt`.
//!
//! This module deliberately starts after repository observation: callers
//! provide a typed before/after delta and the exact attempt policy. Capturing
//! the Git baseline and binding this verdict into the attempt manifest remain
//! separate #2927/#3163 slices.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttemptPathChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct AttemptPathChange {
    pub(crate) path: PathBuf,
    pub(crate) kind: AttemptPathChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) previous_path: Option<PathBuf>,
}

impl AttemptPathChange {
    pub(crate) fn added(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: AttemptPathChangeKind::Added,
            previous_path: None,
        }
    }

    pub(crate) fn modified(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: AttemptPathChangeKind::Modified,
            previous_path: None,
        }
    }

    pub(crate) fn deleted(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            kind: AttemptPathChangeKind::Deleted,
            previous_path: None,
        }
    }

    pub(crate) fn renamed(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        Self {
            path: to.into(),
            kind: AttemptPathChangeKind::Renamed,
            previous_path: Some(from.into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct AttemptDelta {
    /// False when HEAD/worktree/baseline movement makes attribution unsafe.
    pub(crate) comparable: bool,
    pub(crate) changes: Vec<AttemptPathChange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CagePathScope {
    Exact,
    Subtree,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct CagePathRule {
    path: String,
    scope: CagePathScope,
}

impl CagePathRule {
    pub(crate) fn exact(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::new(path.as_ref(), CagePathScope::Exact)
    }

    pub(crate) fn subtree(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::new(path.as_ref(), CagePathScope::Subtree)
    }

    fn new(path: &Path, scope: CagePathScope) -> Result<Self, String> {
        Ok(Self {
            path: normalize_repo_relative_path(path)?,
            scope,
        })
    }

    fn matches(&self, candidate: &str) -> bool {
        match self.scope {
            CagePathScope::Exact => candidate == self.path,
            CagePathScope::Subtree => {
                candidate == self.path
                    || candidate
                        .strip_prefix(&self.path)
                        .is_some_and(|tail| tail.starts_with('/'))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditCagePolicy {
    pub(crate) selected_target: CagePathRule,
    pub(crate) allowed_edit_surface: Vec<CagePathRule>,
    pub(crate) forbidden_paths: Vec<CagePathRule>,
    /// Command-declared generated or receipt writes that may occur alongside
    /// the authored test edit. They never satisfy `selected_target` movement.
    pub(crate) expected_operational_writes: Vec<CagePathRule>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditCageVerdictStatus {
    Compliant,
    Violated,
    Incomparable,
    NotEvaluated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditCageViolationKind {
    InvalidOrEscapingPath,
    ForbiddenPath,
    OutsideAllowedSurface,
    SelectedTargetNotChanged,
    UnexpectedDeletionOrRename,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub(crate) struct EditCageViolation {
    pub(crate) kind: EditCageViolationKind,
    pub(crate) path: String,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditCageVerdict {
    pub(crate) status: EditCageVerdictStatus,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) violations: Vec<EditCageViolation>,
}

pub(crate) fn evaluate_edit_cage(
    policy: &EditCagePolicy,
    delta: &AttemptDelta,
) -> EditCageVerdict {
    if !delta.comparable {
        return EditCageVerdict {
            status: EditCageVerdictStatus::Incomparable,
            changed_paths: Vec::new(),
            violations: Vec::new(),
        };
    }

    let mut changed_paths = Vec::new();
    let mut violations = Vec::new();
    let mut selected_target_changed = false;

    for change in &delta.changes {
        let mut paths = Vec::new();
        if let Some(previous) = change.previous_path.as_ref() {
            paths.push(previous.as_path());
        }
        paths.push(change.path.as_path());

        for raw_path in paths {
            let path = match normalize_repo_relative_path(raw_path) {
                Ok(path) => path,
                Err(reason) => {
                    violations.push(EditCageViolation {
                        kind: EditCageViolationKind::InvalidOrEscapingPath,
                        path: raw_path.to_string_lossy().to_string(),
                        reason,
                    });
                    continue;
                }
            };
            changed_paths.push(path.clone());

            if policy.selected_target.matches(&path) {
                selected_target_changed = true;
                if matches!(
                    change.kind,
                    AttemptPathChangeKind::Deleted | AttemptPathChangeKind::Renamed
                ) {
                    violations.push(EditCageViolation {
                        kind: EditCageViolationKind::UnexpectedDeletionOrRename,
                        path: path.clone(),
                        reason: "the selected repair target was deleted or renamed".to_string(),
                    });
                }
            }

            if policy
                .forbidden_paths
                .iter()
                .any(|rule| rule.matches(&path))
            {
                violations.push(EditCageViolation {
                    kind: EditCageViolationKind::ForbiddenPath,
                    path,
                    reason: "the changed path matches an explicit forbidden rule".to_string(),
                });
                continue;
            }

            let authored_edit_allowed = policy
                .allowed_edit_surface
                .iter()
                .any(|rule| rule.matches(&path));
            let operational_write_allowed = policy
                .expected_operational_writes
                .iter()
                .any(|rule| rule.matches(&path));
            if !authored_edit_allowed && !operational_write_allowed {
                violations.push(EditCageViolation {
                    kind: EditCageViolationKind::OutsideAllowedSurface,
                    path,
                    reason: "the changed path is outside the allowed edit surface and expected operational writes"
                        .to_string(),
                });
            }
        }
    }

    if !selected_target_changed {
        violations.push(EditCageViolation {
            kind: EditCageViolationKind::SelectedTargetNotChanged,
            path: policy.selected_target.path.clone(),
            reason: "the attempt did not change its selected test target".to_string(),
        });
    }

    changed_paths.sort();
    changed_paths.dedup();
    violations.sort();
    violations.dedup();

    EditCageVerdict {
        status: if violations.is_empty() {
            EditCageVerdictStatus::Compliant
        } else {
            EditCageVerdictStatus::Violated
        },
        changed_paths,
        violations,
    }
}

fn normalize_repo_relative_path(path: &Path) -> Result<String, String> {
    let raw = path
        .to_str()
        .ok_or_else(|| "path is not valid UTF-8".to_string())?
        .replace('\\', "/");
    if raw.trim().is_empty() {
        return Err("path is empty".to_string());
    }
    if raw.starts_with('/')
        || raw.starts_with("//")
        || raw
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err("path is rooted or carries a drive/UNC prefix".to_string());
    }

    let mut parts = Vec::new();
    for component in Path::new(&raw).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| "path component is not valid UTF-8".to_string())?;
                if !part.is_empty() {
                    parts.push(part.to_string());
                }
            }
            Component::ParentDir => return Err("path contains parent traversal".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("path is not repository-relative".to_string());
            }
        }
    }
    if parts.is_empty() {
        return Err("path contains no repository-relative component".to_string());
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Result<EditCagePolicy, String> {
        Ok(EditCagePolicy {
            selected_target: CagePathRule::exact("tests/pricing.rs")?,
            allowed_edit_surface: vec![CagePathRule::exact("tests/pricing.rs")?],
            forbidden_paths: vec![
                CagePathRule::subtree("src")?,
                CagePathRule::exact("Cargo.toml")?,
            ],
            expected_operational_writes: vec![CagePathRule::subtree("target/ripr")?],
        })
    }

    #[test]
    fn selected_test_plus_expected_receipt_write_is_compliant() -> Result<(), String> {
        let verdict = evaluate_edit_cage(
            &policy()?,
            &AttemptDelta {
                comparable: true,
                changes: vec![
                    AttemptPathChange::modified("tests\\pricing.rs"),
                    AttemptPathChange::added("target/ripr/reports/agent-receipt.json"),
                ],
            },
        );
        assert_eq!(verdict.status, EditCageVerdictStatus::Compliant);
        assert!(verdict.violations.is_empty());
        assert_eq!(
            verdict.changed_paths,
            vec![
                "target/ripr/reports/agent-receipt.json".to_string(),
                "tests/pricing.rs".to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn forbidden_production_edit_wins_over_other_rules() -> Result<(), String> {
        let verdict = evaluate_edit_cage(
            &policy()?,
            &AttemptDelta {
                comparable: true,
                changes: vec![
                    AttemptPathChange::modified("tests/pricing.rs"),
                    AttemptPathChange::modified("src/pricing.rs"),
                ],
            },
        );
        assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
        assert!(verdict.violations.iter().any(|violation| {
            violation.kind == EditCageViolationKind::ForbiddenPath
                && violation.path == "src/pricing.rs"
        }));
        Ok(())
    }

    #[test]
    fn operational_write_does_not_substitute_for_the_selected_test() -> Result<(), String> {
        let verdict = evaluate_edit_cage(
            &policy()?,
            &AttemptDelta {
                comparable: true,
                changes: vec![AttemptPathChange::added(
                    "target/ripr/reports/agent-receipt.json",
                )],
            },
        );
        assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
        assert!(verdict.violations.iter().any(|violation| {
            violation.kind == EditCageViolationKind::SelectedTargetNotChanged
        }));
        Ok(())
    }

    #[test]
    fn selected_target_delete_or_rename_is_not_a_compliant_repair() -> Result<(), String> {
        for change in [
            AttemptPathChange::deleted("tests/pricing.rs"),
            AttemptPathChange::renamed("tests/pricing.rs", "tests/pricing_new.rs"),
        ] {
            let verdict = evaluate_edit_cage(
                &policy()?,
                &AttemptDelta {
                    comparable: true,
                    changes: vec![change],
                },
            );
            assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
            assert!(verdict.violations.iter().any(|violation| {
                violation.kind == EditCageViolationKind::UnexpectedDeletionOrRename
            }));
        }
        Ok(())
    }

    #[test]
    fn parent_drive_and_unc_paths_fail_closed() -> Result<(), String> {
        for path in ["../tests/pricing.rs", "C:\\tests\\pricing.rs", "\\\\server\\share\\test.rs"] {
            let verdict = evaluate_edit_cage(
                &policy()?,
                &AttemptDelta {
                    comparable: true,
                    changes: vec![AttemptPathChange::modified(path)],
                },
            );
            assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
            assert!(verdict.violations.iter().any(|violation| {
                violation.kind == EditCageViolationKind::InvalidOrEscapingPath
            }));
        }
        Ok(())
    }

    #[test]
    fn incomparable_delta_never_manufactures_compliance() -> Result<(), String> {
        let verdict = evaluate_edit_cage(
            &policy()?,
            &AttemptDelta {
                comparable: false,
                changes: vec![AttemptPathChange::modified("tests/pricing.rs")],
            },
        );
        assert_eq!(verdict.status, EditCageVerdictStatus::Incomparable);
        assert!(verdict.changed_paths.is_empty());
        assert!(verdict.violations.is_empty());
        Ok(())
    }
}

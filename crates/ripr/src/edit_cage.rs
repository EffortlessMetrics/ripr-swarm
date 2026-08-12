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
#[serde(try_from = "AttemptPathChangeWire")]
pub(crate) struct AttemptPathChange {
    path: PathBuf,
    kind: AttemptPathChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_path: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptPathChangeWire {
    path: PathBuf,
    kind: AttemptPathChangeKind,
    previous_path: Option<PathBuf>,
}

impl TryFrom<AttemptPathChangeWire> for AttemptPathChange {
    type Error = String;

    fn try_from(wire: AttemptPathChangeWire) -> Result<Self, Self::Error> {
        match (wire.kind, wire.previous_path) {
            (AttemptPathChangeKind::Renamed, Some(previous_path)) => {
                Ok(Self::renamed(previous_path, wire.path))
            }
            (AttemptPathChangeKind::Renamed, None) => {
                Err("a renamed path change requires previous_path".to_string())
            }
            (kind, Some(_)) => Err(format!(
                "a {} path change must not carry previous_path",
                path_change_kind_name(kind)
            )),
            (kind, None) => Ok(Self::new(wire.path, kind)),
        }
    }
}

impl AttemptPathChange {
    pub(crate) fn added(path: impl Into<PathBuf>) -> Self {
        Self::new(path, AttemptPathChangeKind::Added)
    }

    pub(crate) fn modified(path: impl Into<PathBuf>) -> Self {
        Self::new(path, AttemptPathChangeKind::Modified)
    }

    pub(crate) fn deleted(path: impl Into<PathBuf>) -> Self {
        Self::new(path, AttemptPathChangeKind::Deleted)
    }

    pub(crate) fn renamed(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        Self {
            path: to.into(),
            kind: AttemptPathChangeKind::Renamed,
            previous_path: Some(from.into()),
        }
    }

    fn new(path: impl Into<PathBuf>, kind: AttemptPathChangeKind) -> Self {
        Self {
            path: path.into(),
            kind,
            previous_path: None,
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
#[serde(try_from = "CagePathRuleWire")]
pub(crate) struct CagePathRule {
    path: String,
    scope: CagePathScope,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CagePathRuleWire {
    path: String,
    scope: CagePathScope,
}

impl TryFrom<CagePathRuleWire> for CagePathRule {
    type Error = String;

    fn try_from(wire: CagePathRuleWire) -> Result<Self, Self::Error> {
        Self::new(Path::new(&wire.path), wire.scope)
    }
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditCageViolationKind {
    InvalidOrEscapingPath,
    InvalidDelta,
    InvalidPolicy,
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

pub(crate) fn evaluate_edit_cage(policy: &EditCagePolicy, delta: &AttemptDelta) -> EditCageVerdict {
    let delta_violations = validate_delta(delta);
    if !delta.comparable || !delta_violations.is_empty() {
        let (changed_paths, mut violations) = normalized_audit_paths(delta);
        violations.extend(delta_violations);
        violations.extend(validate_policy(policy));
        violations.sort();
        violations.dedup();
        return EditCageVerdict {
            status: EditCageVerdictStatus::Incomparable,
            changed_paths,
            violations,
        };
    }

    let policy_violations = validate_policy(policy);
    if !policy_violations.is_empty() {
        let (changed_paths, mut violations) = normalized_audit_paths(delta);
        violations.extend(policy_violations);
        violations.sort();
        violations.dedup();
        return EditCageVerdict {
            status: EditCageVerdictStatus::Incomparable,
            changed_paths,
            violations,
        };
    }

    let mut changed_paths = Vec::new();
    let mut violations = Vec::new();
    let mut selected_target_changed = false;

    for change in &delta.changes {
        evaluate_change(
            policy,
            change,
            &mut selected_target_changed,
            &mut changed_paths,
            &mut violations,
        );
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

fn validate_delta(delta: &AttemptDelta) -> Vec<EditCageViolation> {
    delta
        .changes
        .iter()
        .filter_map(|change| {
            let shape_is_valid = matches!(
                (change.kind, change.previous_path.is_some()),
                (AttemptPathChangeKind::Renamed, true)
                    | (
                        AttemptPathChangeKind::Added
                            | AttemptPathChangeKind::Modified
                            | AttemptPathChangeKind::Deleted,
                        false
                    )
            );
            (!shape_is_valid).then(|| EditCageViolation {
                kind: EditCageViolationKind::InvalidDelta,
                path: change.path.to_string_lossy().to_string(),
                reason: if change.kind == AttemptPathChangeKind::Renamed {
                    "a renamed path change requires previous_path".to_string()
                } else {
                    format!(
                        "a {} path change must not carry previous_path",
                        path_change_kind_name(change.kind)
                    )
                },
            })
        })
        .collect()
}

fn evaluate_change(
    policy: &EditCagePolicy,
    change: &AttemptPathChange,
    selected_target_changed: &mut bool,
    changed_paths: &mut Vec<String>,
    violations: &mut Vec<EditCageViolation>,
) {
    let paths = change
        .previous_path
        .iter()
        .map(|path| {
            (
                path.as_path(),
                change.kind == AttemptPathChangeKind::Renamed,
                change.kind == AttemptPathChangeKind::Renamed,
            )
        })
        .chain(std::iter::once((
            change.path.as_path(),
            change.kind == AttemptPathChangeKind::Deleted,
            change.kind != AttemptPathChangeKind::Renamed,
        )));
    for (raw_path, is_removed_side, is_target_movement) in paths {
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
        evaluate_normalized_path(
            policy,
            path,
            is_removed_side,
            is_target_movement,
            selected_target_changed,
            violations,
        );
    }
}

fn evaluate_normalized_path(
    policy: &EditCagePolicy,
    path: String,
    is_removed_side: bool,
    is_target_movement: bool,
    selected_target_changed: &mut bool,
    violations: &mut Vec<EditCageViolation>,
) {
    if policy.selected_target.matches(&path) {
        *selected_target_changed |= is_target_movement;
        if is_removed_side {
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
        return;
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

fn validate_policy(policy: &EditCagePolicy) -> Vec<EditCageViolation> {
    let mut violations = Vec::new();
    let selected_path = policy.selected_target.path.clone();
    if policy.selected_target.scope != CagePathScope::Exact {
        violations.push(invalid_policy_violation(
            &selected_path,
            "the selected target must be one exact repository-relative path",
        ));
    }
    if !policy
        .allowed_edit_surface
        .iter()
        .any(|rule| rule.matches(&selected_path))
    {
        violations.push(invalid_policy_violation(
            &selected_path,
            "the selected target is outside the authored edit surface",
        ));
    }
    if policy
        .expected_operational_writes
        .iter()
        .any(|rule| rule.matches(&selected_path))
    {
        violations.push(invalid_policy_violation(
            &selected_path,
            "the selected target overlaps an expected operational write",
        ));
    }
    if policy
        .forbidden_paths
        .iter()
        .any(|rule| rule.matches(&selected_path))
    {
        violations.push(invalid_policy_violation(
            &selected_path,
            "the selected target overlaps an explicit forbidden path",
        ));
    }
    violations
}

fn invalid_policy_violation(path: &str, reason: &str) -> EditCageViolation {
    EditCageViolation {
        kind: EditCageViolationKind::InvalidPolicy,
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

fn normalized_audit_paths(delta: &AttemptDelta) -> (Vec<String>, Vec<EditCageViolation>) {
    let mut changed_paths = Vec::new();
    let mut violations = Vec::new();
    for raw_path in delta.changes.iter().flat_map(|change| {
        change
            .previous_path
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(change.path.as_path()))
    }) {
        match normalize_repo_relative_path(raw_path) {
            Ok(path) => changed_paths.push(path),
            Err(reason) => violations.push(EditCageViolation {
                kind: EditCageViolationKind::InvalidOrEscapingPath,
                path: raw_path.to_string_lossy().to_string(),
                reason,
            }),
        }
    }
    changed_paths.sort();
    changed_paths.dedup();
    violations.sort();
    violations.dedup();
    (changed_paths, violations)
}

fn path_change_kind_name(kind: AttemptPathChangeKind) -> &'static str {
    match kind {
        AttemptPathChangeKind::Added => "added",
        AttemptPathChangeKind::Modified => "modified",
        AttemptPathChangeKind::Deleted => "deleted",
        AttemptPathChangeKind::Renamed => "renamed",
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
    use serde::de::DeserializeOwned;

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

    fn require_invalid_json<T: DeserializeOwned>(raw: &str) -> Result<(), String> {
        match serde_json::from_str::<T>(raw) {
            Ok(_) => Err(format!("expected invalid JSON input to be rejected: {raw}")),
            Err(_) => Ok(()),
        }
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
    fn rename_into_selected_target_cannot_substitute_without_baseline_authority()
    -> Result<(), String> {
        let mut rename_policy = policy()?;
        rename_policy.allowed_edit_surface = vec![CagePathRule::subtree("tests")?];
        let verdict = evaluate_edit_cage(
            &rename_policy,
            &AttemptDelta {
                comparable: true,
                changes: vec![AttemptPathChange::renamed(
                    "tests/other.rs",
                    "tests/pricing.rs",
                )],
            },
        );

        assert_eq!(verdict.status, EditCageVerdictStatus::Violated);
        assert_eq!(
            verdict.changed_paths,
            vec!["tests/other.rs".to_string(), "tests/pricing.rs".to_string()]
        );
        assert!(verdict.violations.iter().any(|violation| {
            violation.kind == EditCageViolationKind::SelectedTargetNotChanged
        }));
        assert!(!verdict.violations.iter().any(|violation| {
            violation.kind == EditCageViolationKind::UnexpectedDeletionOrRename
                && violation.path == "tests/pricing.rs"
        }));
        Ok(())
    }

    #[test]
    fn subtree_selected_target_is_rejected_as_an_inexact_policy() -> Result<(), String> {
        let subtree_policy = EditCagePolicy {
            selected_target: CagePathRule::subtree("tests/pricing")?,
            allowed_edit_surface: vec![CagePathRule::subtree("tests/pricing")?],
            forbidden_paths: Vec::new(),
            expected_operational_writes: Vec::new(),
        };
        let verdict = evaluate_edit_cage(
            &subtree_policy,
            &AttemptDelta {
                comparable: true,
                changes: vec![AttemptPathChange::modified("tests/pricing/boundary.rs")],
            },
        );

        assert_eq!(verdict.status, EditCageVerdictStatus::Incomparable);
        assert_eq!(
            verdict.changed_paths,
            vec!["tests/pricing/boundary.rs".to_string()]
        );
        assert!(verdict.violations.iter().any(|violation| {
            violation.kind == EditCageViolationKind::InvalidPolicy
                && violation.reason.contains("one exact")
        }));
        Ok(())
    }

    #[test]
    fn selected_target_must_be_authored_and_disjoint_from_other_roles() -> Result<(), String> {
        let mut outside_authored_surface = policy()?;
        outside_authored_surface.allowed_edit_surface =
            vec![CagePathRule::exact("tests/other.rs")?];

        let mut operational_target = policy()?;
        operational_target.expected_operational_writes =
            vec![CagePathRule::exact("tests/pricing.rs")?];

        let mut forbidden_target = policy()?;
        forbidden_target
            .forbidden_paths
            .push(CagePathRule::exact("tests/pricing.rs")?);

        for invalid_policy in [
            outside_authored_surface,
            operational_target,
            forbidden_target,
        ] {
            let verdict = evaluate_edit_cage(
                &invalid_policy,
                &AttemptDelta {
                    comparable: true,
                    changes: vec![AttemptPathChange::modified("tests/pricing.rs")],
                },
            );
            assert_eq!(verdict.status, EditCageVerdictStatus::Incomparable);
            assert_eq!(verdict.changed_paths, vec!["tests/pricing.rs".to_string()]);
            assert!(
                verdict
                    .violations
                    .iter()
                    .any(|violation| violation.kind == EditCageViolationKind::InvalidPolicy)
            );
        }
        Ok(())
    }

    #[test]
    fn deserialized_path_rules_reuse_the_repository_path_validator() -> Result<(), String> {
        let normalized =
            serde_json::from_str::<CagePathRule>(r#"{"path":"tests\\pricing.rs","scope":"exact"}"#)
                .map_err(|error| error.to_string())?;
        assert_eq!(normalized.path, "tests/pricing.rs");
        assert_eq!(normalized.scope, CagePathScope::Exact);

        for invalid in [
            r#"{"path":"../tests/pricing.rs","scope":"exact"}"#,
            r#"{"path":"/tests/pricing.rs","scope":"exact"}"#,
        ] {
            require_invalid_json::<CagePathRule>(invalid)?;
        }
        let drive_path = format!("{}:\\tests\\pricing.rs", 'C');
        let drive_rule = serde_json::json!({"path": drive_path, "scope": "exact"});
        let drive_rule = serde_json::to_string(&drive_rule).map_err(|error| error.to_string())?;
        require_invalid_json::<CagePathRule>(&drive_rule)?;
        Ok(())
    }

    #[test]
    fn deserialized_path_changes_reject_incoherent_rename_shapes() -> Result<(), String> {
        let renamed = serde_json::from_str::<AttemptPathChange>(
            r#"{"path":"tests/new.rs","kind":"renamed","previous_path":"tests/old.rs"}"#,
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(
            renamed,
            AttemptPathChange::renamed("tests/old.rs", "tests/new.rs")
        );

        for invalid in [
            r#"{"path":"tests/new.rs","kind":"renamed"}"#,
            r#"{"path":"tests/new.rs","kind":"modified","previous_path":"tests/old.rs"}"#,
        ] {
            require_invalid_json::<AttemptPathChange>(invalid)?;
        }
        Ok(())
    }

    #[test]
    fn malformed_in_memory_change_shape_cannot_fabricate_target_movement() -> Result<(), String> {
        let mut authored_tests_policy = policy()?;
        authored_tests_policy.allowed_edit_surface = vec![CagePathRule::subtree("tests")?];
        let malformed_change = AttemptPathChange {
            path: PathBuf::from("tests/other.rs"),
            kind: AttemptPathChangeKind::Modified,
            previous_path: Some(PathBuf::from("tests/pricing.rs")),
        };

        let verdict = evaluate_edit_cage(
            &authored_tests_policy,
            &AttemptDelta {
                comparable: true,
                changes: vec![malformed_change],
            },
        );

        assert_eq!(verdict.status, EditCageVerdictStatus::Incomparable);
        assert_eq!(
            verdict.changed_paths,
            vec!["tests/other.rs".to_string(), "tests/pricing.rs".to_string()]
        );
        assert!(verdict.violations.iter().any(|violation| {
            violation.kind == EditCageViolationKind::InvalidDelta
                && violation.path == "tests/other.rs"
        }));
        Ok(())
    }

    #[test]
    fn parent_drive_and_unc_paths_fail_closed() -> Result<(), String> {
        let drive_path = format!("{}:\\tests\\pricing.rs", 'C');
        let unc_path = ["", "", "server", "share", "test.rs"].join("\\");
        for path in ["../tests/pricing.rs".to_string(), drive_path, unc_path] {
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
        assert_eq!(verdict.changed_paths, vec!["tests/pricing.rs".to_string()]);
        assert!(verdict.violations.is_empty());
        Ok(())
    }
}

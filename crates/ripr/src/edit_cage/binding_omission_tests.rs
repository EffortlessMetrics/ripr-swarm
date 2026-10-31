//! Identity omission is custody, not directory membership. These pure controls
//! supply captured path states to the actual omission owners; Git capture and
//! full CLI currentness are exercised separately by the installed journeys.
use super::*;

fn baseline(store: &str) -> Result<AttemptBaseline, String> {
    Ok(AttemptBaseline {
        root: PathBuf::from("."),
        head: "a".repeat(40),
        policy: EditCagePolicy {
            selected_target: CagePathRule::exact("tests/focused.rs")?,
            allowed_edit_surface: vec![CagePathRule::exact("tests/focused.rs")?],
            forbidden_paths: vec![CagePathRule::subtree("src")?],
            expected_operational_writes: vec![
                CagePathRule::subtree("target/ripr")?,
                CagePathRule::subtree(store)?,
            ],
            ignored_build_output: None,
            untracked_build_lockfile: None,
            inline_test_module_target: false,
        },
        paths: BTreeMap::new(),
        ambiguous: false,
        inline_test_region: None,
        #[cfg(windows)]
        _unknown_ignored_write_guards: Vec::new(),
        #[cfg(windows)]
        _expected_write_authorities: Vec::new(),
    })
}

fn captured_file(digest: &str) -> RepositoryPathState {
    RepositoryPathState {
        index_entry: None,
        worktree_identity: WorktreeIdentity::File {
            digest: digest.to_string(),
            executable: false,
            eol_normalized_digest: None,
        },
    }
}

fn captured_change(path: &str, digest: &str) -> AttemptPathChange {
    let mut change = AttemptPathChange::added(path);
    change.after_state = Some(captured_file(digest));
    change
}

fn captured_delta(changes: Vec<AttemptPathChange>) -> AttemptDelta {
    AttemptDelta {
        comparable: true,
        changes,
        inline_test_region: None,
    }
}

fn assert_non_owned_file_remains_bound(store: &str, directory: &str) -> Result<(), String> {
    let baseline = baseline(store)?;
    let manifest = format!("{store}/selected/attempt.json");
    let neighbor = format!("{directory}/not-owned-by-this-invocation.json");
    let backup = "target/ripr/workflow/after.json.ripr-preattempt-backup";
    let bound_paths = vec![manifest.clone(), neighbor.clone()];
    let bind = |digest: &str| -> Result<AttemptDelta, String> {
        let mut delta = captured_delta(vec![
            captured_change(&manifest, "manifest"),
            captured_change(&neighbor, digest),
            captured_change(backup, "owned-copy"),
        ]);
        omit_attempt_manifest_identity(&mut delta, &baseline, Path::new(&manifest))?;
        omit_owned_workflow_backups(
            &mut delta,
            &baseline,
            &[(backup.into(), "owned-copy".into(), false)],
        )?;
        // Recalculation may discard only newly appearing operational writes,
        // not identities of paths already present in the bound verdict.
        Ok(without_later_operational_writes(
            baseline.policy(),
            &delta,
            &bound_paths,
        ))
    };
    let bound = bind("before-rewrite")?;
    let later = bind("after-rewrite")?;
    assert_eq!(bound.changes.len(), 2, "only the exact owned backup leaves");
    assert_eq!(
        bound.changes[0].after_state, None,
        "self identity is omitted"
    );
    assert_eq!(
        bound.changes[1].after_state,
        Some(captured_file("before-rewrite")),
        "non-owned operational identity was hidden in {directory}"
    );
    assert_eq!(bound.changes[1].path, later.changes[1].path);
    assert_eq!(bound.changes[1].kind, later.changes[1].kind);
    assert_ne!(bound, later, "same-path byte drift must remain bound");
    Ok(())
}

#[test]
fn non_owned_attempt_store_file_remains_bound() -> Result<(), String> {
    assert_non_owned_file_remains_bound(
        "target/ripr/repair-attempts",
        "target/ripr/repair-attempts",
    )
}

#[test]
fn non_owned_explicit_store_file_remains_bound() -> Result<(), String> {
    assert_non_owned_file_remains_bound("custom-attempts", "custom-attempts")
}

#[test]
fn non_owned_workflow_file_remains_bound() -> Result<(), String> {
    assert_non_owned_file_remains_bound("target/ripr/repair-attempts", "target/ripr/workflow")
}

#[test]
fn non_owned_report_file_remains_bound() -> Result<(), String> {
    assert_non_owned_file_remains_bound("target/ripr/repair-attempts", "target/ripr/reports")
}

#[test]
fn non_owned_cache_file_remains_bound() -> Result<(), String> {
    assert_non_owned_file_remains_bound("target/ripr/repair-attempts", "target/ripr/cache")
}

#[test]
fn selected_manifest_exception_never_spreads_to_sibling_eligibility_classes() -> Result<(), String>
{
    let store = "target/ripr/repair-attempts";
    let selected = format!("{store}/selected/attempt.json");
    let sibling = format!("{store}/sibling/attempt.json");
    let mut hidden = Vec::new();
    for eligibility in [
        "operational",
        "selected",
        "authored",
        "forbidden",
        "baseline_index",
        "current_index",
        "not_operational",
    ] {
        let mut baseline = baseline(store)?;
        let mut sibling_change = captured_change(&sibling, "sibling-evidence");
        match eligibility {
            "selected" => baseline.policy.selected_target = CagePathRule::exact(&sibling)?,
            "authored" => baseline
                .policy
                .allowed_edit_surface
                .push(CagePathRule::exact(&sibling)?),
            "forbidden" => baseline
                .policy
                .forbidden_paths
                .push(CagePathRule::exact(&sibling)?),
            "baseline_index" => {
                let mut tracked = captured_file("baseline-sibling");
                tracked.index_entry = Some("100644 baseline-object 0".into());
                baseline.paths.insert(sibling.clone(), tracked);
            }
            "current_index" => {
                sibling_change
                    .after_state
                    .as_mut()
                    .ok_or("missing capture")?
                    .index_entry = Some("100644 current-object 0".into())
            }
            "not_operational" => {
                baseline.policy.expected_operational_writes = vec![CagePathRule::exact(&selected)?]
            }
            _ => {}
        }
        let expected = sibling_change.clone();
        let mut delta = captured_delta(vec![captured_change(&selected, "self"), sibling_change]);
        omit_attempt_manifest_identity(&mut delta, &baseline, Path::new(&selected))?;
        assert_eq!(
            delta.changes[0].after_state, None,
            "selected manifest positive control"
        );
        if delta.changes[1] != expected {
            hidden.push(eligibility);
        }
    }
    assert!(
        hidden.is_empty(),
        "sibling identities wrongly omitted: {hidden:?}"
    );
    Ok(())
}

fn receipt_change(bytes: &[u8]) -> AttemptPathChange {
    captured_change(RECEIPT_PROJECTION, &format!("{:x}", Sha256::digest(bytes)))
}

#[test]
fn issued_receipt_replacement_reconstructs_only_the_exact_bound_slot() -> Result<(), String> {
    let baseline = baseline("target/ripr/repair-attempts")?;
    let prior = b"issued A\r\n";
    let issued = b"issued B\n";
    let mut old = receipt_change(prior);
    if let Some(RepositoryPathState {
        worktree_identity:
            WorktreeIdentity::File {
                eol_normalized_digest,
                ..
            },
        ..
    }) = &mut old.after_state
    {
        *eol_normalized_digest = eol_normalized_file_digest(prior);
    }
    let neighbor = captured_change("target/ripr/reports/neighbor.json", "bound-neighbor");
    let sibling = captured_change(
        "target/ripr/repair-attempts/other/attempt.json",
        "bound-sibling",
    );
    let bound = captured_delta(vec![old, neighbor.clone(), sibling.clone()]);
    assert_eq!(
        receipt_projection_identity(&bound, &baseline, prior),
        Some((AttemptPathChangeKind::Added, false))
    );
    let mut current = captured_delta(vec![receipt_change(issued), neighbor, sibling]);
    assert_ne!(bound, current);
    assert!(reconstruct_receipt_projection(
        &mut current,
        &baseline,
        AttemptPathChangeKind::Added,
        false,
        prior,
        issued
    ));
    assert_eq!(bound, current, "only the issued projection moves backward");
    for index in [1, 2] {
        let mut drifted = captured_delta(vec![
            receipt_change(issued),
            bound.changes[1].clone(),
            bound.changes[2].clone(),
        ]);
        drifted.changes[index].after_state = Some(captured_file("external drift"));
        assert!(reconstruct_receipt_projection(
            &mut drifted,
            &baseline,
            AttemptPathChangeKind::Added,
            false,
            prior,
            issued
        ));
        assert_ne!(
            bound, drifted,
            "neighbor and sibling drift must still break the whole-delta binding"
        );
    }
    Ok(())
}

#[test]
fn receipt_replacement_refuses_unowned_bytes_modes_shapes_and_policy() -> Result<(), String> {
    for guard in [
        "different_bytes",
        "executable",
        "kind",
        "baseline_index",
        "current_index",
        "selected",
        "authored",
        "forbidden",
        "not_operational",
        "symlink",
        "incomparable",
    ] {
        let mut baseline = baseline("target/ripr/repair-attempts")?;
        let mut delta = captured_delta(vec![receipt_change(b"issued B")]);
        match guard {
            "different_bytes" => delta.changes[0] = receipt_change(b"unowned B"),
            "executable" => {
                if let Some(RepositoryPathState {
                    worktree_identity: WorktreeIdentity::File { executable, .. },
                    ..
                }) = &mut delta.changes[0].after_state
                {
                    *executable = true;
                }
            }
            "kind" => delta.changes[0].kind = AttemptPathChangeKind::Modified,
            "baseline_index" => {
                let mut state = captured_file("baseline");
                state.index_entry = Some("100644 object 0".into());
                baseline.paths.insert(RECEIPT_PROJECTION.into(), state);
            }
            "current_index" => {
                delta.changes[0]
                    .after_state
                    .as_mut()
                    .ok_or("missing state")?
                    .index_entry = Some("100644 object 0".into())
            }
            "selected" => {
                baseline.policy.selected_target = CagePathRule::exact(RECEIPT_PROJECTION)?
            }
            "authored" => baseline
                .policy
                .allowed_edit_surface
                .push(CagePathRule::exact(RECEIPT_PROJECTION)?),
            "forbidden" => baseline
                .policy
                .forbidden_paths
                .push(CagePathRule::exact(RECEIPT_PROJECTION)?),
            "not_operational" => baseline.policy.expected_operational_writes.clear(),
            "symlink" => {
                delta.changes[0]
                    .after_state
                    .as_mut()
                    .ok_or("missing state")?
                    .worktree_identity = WorktreeIdentity::Symlink("other".into())
            }
            "incomparable" => delta.comparable = false,
            _ => return Err("unknown guard".into()),
        }
        let unchanged = delta.clone();
        assert!(
            !reconstruct_receipt_projection(
                &mut delta,
                &baseline,
                AttemptPathChangeKind::Added,
                false,
                b"issued A",
                b"issued B"
            ),
            "{guard}"
        );
        assert_eq!(
            delta, unchanged,
            "a refused reconstruction cannot alter other evidence"
        );
    }
    Ok(())
}

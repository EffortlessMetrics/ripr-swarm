use super::*;

const SKIPPED: &str = "#[mutants::skip]\npub fn skipped_state(flag: bool) -> bool { !flag }\n\npub fn live_state(flag: bool) -> bool { !flag }\n";
const UNATTRIBUTED: &str = "pub fn skipped_state(flag: bool) -> bool { !flag }\n\npub fn live_state(flag: bool) -> bool { !flag }\n";

fn commit_base(root: &Path) -> Result<(), String> {
    write_lsp_scope_fixture(root)?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn skipped_state(flag: bool) -> bool { flag }\n\npub fn live_state(flag: bool) -> bool { flag }\n",
    )
    .map_err(|err| format!("write base source failed: {err}"))?;
    run_lsp_scope_git(root, &["init"])?;
    run_lsp_scope_git(root, &["config", "user.email", "ripr@example.invalid"])?;
    run_lsp_scope_git(root, &["config", "user.name", "RIPR Test"])?;
    run_lsp_scope_git(
        root,
        &["add", "Cargo.toml", "src/lib.rs", "tests/end_to_end.rs"],
    )?;
    run_lsp_scope_git(root, &["commit", "-m", "base"])
}

fn config() -> LspAnalysisConfig {
    LspAnalysisConfig {
        base_ref: Some("HEAD".to_string()),
        mode: Mode::Instant,
        diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
        ..LspAnalysisConfig::default()
    }
}

fn published_lines(diagnostics: &crate::lsp::diagnostics::WorkspaceDiagnostics) -> Vec<u32> {
    let mut lines: Vec<u32> = diagnostics
        .batches
        .iter()
        .flat_map(|batch| batch.diagnostics.iter().map(|d| d.range.start.line))
        .collect();
    lines.sort_unstable();
    lines
}

/// #14136: the editor withholds exactly the findings `ripr check` withholds for
/// a `#[mutants::skip]` owner, discloses the count, and recovers them when the
/// attribute is removed.
#[test]
fn mutants_skip_owner_findings_are_withheld_from_the_editor_and_return_with_the_attribute_gone()
-> Result<(), String> {
    let root = unique_lsp_test_root("owner-opt-out")?;
    commit_base(root.path())?;
    let config = config();
    let source = root.path().join("src/lib.rs");

    // Unattributed control: both changed functions publish a diagnostic.
    std::fs::write(&source, UNATTRIBUTED).map_err(|err| format!("write control: {err}"))?;
    let control =
        crate::lsp::diagnostics::workspace_diagnostics_with_config(root.path(), &config, true)?;
    let control_lines = published_lines(&control);
    if control_lines.len() != 2 || control.snapshot.owner_opt_out_findings != 0 {
        return Err(format!(
            "SETUP: control must publish both changed functions without a disclosure, got lines {control_lines:?} and {} withheld",
            control.snapshot.owner_opt_out_findings
        ));
    }

    // Attributed: the skipped owner's finding leaves every projection.
    std::fs::write(&source, SKIPPED).map_err(|err| format!("write attributed: {err}"))?;
    let withheld =
        crate::lsp::diagnostics::workspace_diagnostics_with_config(root.path(), &config, true)?;
    // The same saved-worktree check the refresh runs, so the id set under test
    // is the one `ripr check --worktree` withholds for this tree.
    let (check, _, _, _) =
        crate::app::check_workspace_worktree_with_sources_open_rust_paths_and_progress(
            config.check_input(root.path()),
            config.repo_config(),
            &std::collections::BTreeSet::new(),
            None,
        )
        .map_err(|err| format!("check run failed: {err}"))?;
    let check_withheld: std::collections::BTreeSet<String> = check
        .withheld_finding_ids()
        .into_iter()
        .map(str::to_string)
        .collect();
    if check_withheld.is_empty() {
        return Err("SETUP: ripr check withheld nothing for the attributed owner".into());
    }
    let projected: std::collections::BTreeSet<String> = withheld
        .snapshot
        .findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect();
    if projected.iter().any(|id| check_withheld.contains(id)) {
        return Err(format!(
            "the editor projected a finding ripr check withholds: {projected:?} vs {check_withheld:?}"
        ));
    }
    let every: std::collections::BTreeSet<String> = check
        .findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect();
    if projected != every.difference(&check_withheld).cloned().collect() {
        return Err(format!(
            "the editor must project exactly the findings ripr check keeps: {projected:?} vs {every:?} minus {check_withheld:?}"
        ));
    }
    let lines = published_lines(&withheld);
    if lines != vec![3] {
        return Err(format!(
            "only the unattributed function (line 4) may publish, got lines {lines:?}"
        ));
    }
    if withheld.snapshot.owner_opt_out_findings != check_withheld.len() {
        return Err(format!(
            "disclosure count {} must equal the withheld set size {}",
            withheld.snapshot.owner_opt_out_findings,
            check_withheld.len()
        ));
    }

    // Removing the attribute and refreshing brings the diagnostic back.
    std::fs::write(&source, UNATTRIBUTED).map_err(|err| format!("write restored: {err}"))?;
    let restored =
        crate::lsp::diagnostics::workspace_diagnostics_with_config(root.path(), &config, true)?;
    if published_lines(&restored) != control_lines || restored.snapshot.owner_opt_out_findings != 0
    {
        return Err("withheld state remained after removing the attribute".into());
    }
    Ok(())
}

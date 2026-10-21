use super::*;

/// Recovery must replay the failed analysis even when the dirty default
/// selects a different, nonempty set of findings.
#[test]
fn selector_recovery_replays_committed_worktree_and_supplied_diff_scopes() -> Result<(), String> {
    let root = unique_temp_workspace("selector-recovery-scope");
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("create src: {err}"))?;
    run_git(&root, &["init"])?;
    run_git(&root, &["config", "user.email", "test@test.com"])?;
    run_git(&root, &["config", "user.name", "Test"])?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"selector-recovery-scope\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    ).map_err(|err| format!("write manifest: {err}"))?;
    std::fs::write(root.join("src/lib.rs"), "mod committed;\nmod worktree;\n")
        .map_err(|err| format!("write lib: {err}"))?;
    let write = |name: &str, operator: &str| {
        std::fs::write(
            root.join(format!("src/{name}.rs")),
            format!("pub fn {name}(n: i32) -> bool {{\n    n {operator} 10\n}}\n"),
        ).map_err(|err| format!("write {name}: {err}"))
    };
    write("committed", ">")?;
    write("worktree", ">")?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "base"])?;
    write("committed", ">=")?;
    run_git(&root, &["commit", "-am", "committed boundary"])?;
    let patch = common::fixture_git::fixture_git_output(&root, &["diff", "HEAD~1", "HEAD"])?;
    let patch_path = root.join("saved.patch");
    std::fs::write(&patch_path, patch).map_err(|err| format!("write patch: {err}"))?;
    write("committed", ">")?;
    write("worktree", ">=")?;
    let root_str = root.to_string_lossy().into_owned();
    let patch_str = patch_path.to_string_lossy().into_owned();
    let list = |scope: &[&str]| {
        let mut args = vec!["check", "--root", &root_str];
        args.extend_from_slice(scope);
        args.push("--json");
        let output = run_ripr(&args);
        assert_success(&output);
        first_finding_id(&output.stdout)
    };
    let committed = list(&["--base", "HEAD~1", "--committed"])?;
    let worktree = list(&["--base", "HEAD~1", "--worktree"])?;
    if committed == worktree || !committed.contains("committed") || !worktree.contains("worktree") {
        return Err(format!("fixture must have opposing nonempty findings: {committed}, {worktree}"));
    }
    if list(&["--base", "HEAD~1"])? != worktree {
        return Err("dirty default must select the worktree control".to_string());
    }
    // Preserved routes execute before the committed-scope regression.
    for (label, scope, expected, excluded) in [
        ("worktree", vec!["--base", "HEAD~1", "--worktree"], &worktree, &committed),
        ("supplied diff", vec!["--diff", &patch_str], &committed, &worktree),
        ("committed", vec!["--base", "HEAD~1"], &committed, &worktree),
    ] {
        for command in ["explain", "context"] {
            let mut args = vec![command, "--root", &root_str];
            args.extend_from_slice(&scope);
            if command == "context" { args.push("--at"); }
            args.push("probe:no-such-finding");
            let miss = run_ripr(&args);
            if miss.status.code() != Some(2) || !miss.stdout.is_empty() {
                return Err(format!("{label} {command} miss contract changed: {miss:?}"));
            }
            let stderr = String::from_utf8_lossy(&miss.stderr);
            let emitted = stderr.split('`').find(|text| text.starts_with("ripr check "))
                .ok_or_else(|| format!("{label} {command} emitted no recovery: {stderr}"))?;
            let replay_args = printed_ripr_args(emitted)?;
            let replay_args: Vec<&str> = replay_args.iter().map(String::as_str).collect();
            let replay = run_ripr(&replay_args);
            assert_success(&replay);
            let json: serde_json::Value = serde_json::from_slice(&replay.stdout)
                .map_err(|err| format!("parse replay: {err}"))?;
            let findings = json["findings"].as_array()
                .ok_or_else(|| format!("replay has no findings array: {json}"))?;
            if !findings.iter().any(|f| f["id"].as_str() == Some(expected.as_str()))
                || findings.iter().any(|f| f["id"].as_str() == Some(excluded.as_str())) {
                return Err(format!("{label} {command} recovery changed subject: `{emitted}`; expected {expected}, excluded {excluded}; {json}"));
            }
            if label == "committed" && !replay_args.contains(&"--committed") {
                return Err(format!("committed recovery must pin its diff source: {emitted}"));
            }
        }
    }
    ignore_remove_dir_all(&root);
    Ok(())
}


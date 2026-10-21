use super::*;

struct SelectorScopeFixture {
    root: PathBuf,
    patch: String,
    committed: String,
    worktree: String,
}

fn selector_scope_fixture() -> Result<SelectorScopeFixture, String> {
    let root = unique_temp_workspace("selector-recovery-scope");
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("create src: {err}"))?;
    run_git(&root, &["init"])?;
    run_git(&root, &["config", "user.email", "test@test.com"])?;
    run_git(&root, &["config", "user.name", "Test"])?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"selector-recovery-scope\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|err| format!("write manifest: {err}"))?;
    std::fs::write(root.join("src/lib.rs"), "mod committed;\nmod worktree;\n")
        .map_err(|err| format!("write lib: {err}"))?;
    let write = |name: &str, operator: &str| {
        std::fs::write(
            root.join(format!("src/{name}.rs")),
            format!("pub fn {name}(n: i32) -> bool {{\n    n {operator} 10\n}}\n"),
        )
        .map_err(|err| format!("write {name}: {err}"))
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
    if committed != "probe:src_committed.rs:predicate:cccbf91e"
        || worktree != "probe:src_worktree.rs:predicate:70c58575"
    {
        return Err(format!(
            "fixture must have opposing nonempty findings: {committed}, {worktree}"
        ));
    }
    if list(&["--base", "HEAD~1"])? != worktree {
        return Err("dirty default must select the worktree control".to_string());
    }
    Ok(SelectorScopeFixture {
        root,
        patch: patch_str,
        committed,
        worktree,
    })
}

/// Recovery must replay the failed analysis even when the dirty default
/// selects a different, nonempty set of findings.
#[test]
fn selector_recovery_replays_committed_worktree_and_supplied_diff_scopes() -> Result<(), String> {
    let SelectorScopeFixture {
        root,
        patch: patch_str,
        committed,
        worktree,
    } = selector_scope_fixture()?;
    let root_str = root.to_string_lossy().into_owned();
    // Preserved routes execute before the committed-scope regression.
    for (label, scope, expected, excluded) in [
        (
            "worktree",
            vec!["--base", "HEAD~1", "--worktree"],
            &worktree,
            &committed,
        ),
        (
            "supplied diff",
            vec!["--diff", &patch_str],
            &committed,
            &worktree,
        ),
        (
            "dirty default",
            vec!["--base", "HEAD~1"],
            &committed,
            &worktree,
        ),
        (
            "committed",
            vec!["--base", "HEAD~1", "--committed"],
            &committed,
            &worktree,
        ),
    ] {
        for command in ["explain", "context"] {
            let mut args = vec![command, "--root", &root_str];
            args.extend_from_slice(&scope);
            if command == "context" {
                args.push("--at");
            }
            args.push("probe:no-such-finding");
            let miss = run_ripr(&args);
            if miss.status.code() != Some(2) || !miss.stdout.is_empty() {
                return Err(format!("{label} {command} miss contract changed: {miss:?}"));
            }
            let stderr = String::from_utf8_lossy(&miss.stderr);
            let emitted = stderr
                .split('`')
                .find(|text| text.starts_with("ripr check "))
                .ok_or_else(|| format!("{label} {command} emitted no recovery: {stderr}"))?;
            let replay_args = printed_ripr_args(emitted)?;
            let replay_args: Vec<&str> = replay_args.iter().map(String::as_str).collect();
            let replay = run_ripr(&replay_args);
            assert_success(&replay);
            let json: serde_json::Value = serde_json::from_slice(&replay.stdout)
                .map_err(|err| format!("parse replay: {err}"))?;
            let findings = json["findings"]
                .as_array()
                .ok_or_else(|| format!("replay has no findings array: {json}"))?;
            if !findings
                .iter()
                .any(|f| f["id"].as_str() == Some(expected.as_str()))
                || findings
                    .iter()
                    .any(|f| f["id"].as_str() == Some(excluded.as_str()))
            {
                return Err(format!(
                    "{label} {command} recovery changed subject: `{emitted}`; expected {expected}, excluded {excluded}; {json}"
                ));
            }
            if label == "committed" && !replay_args.contains(&"--committed") {
                return Err(format!(
                    "committed recovery must pin its diff source: {emitted}"
                ));
            }
        }
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

fn printed_drill_in_scope(staged: bool) -> Result<(), String> {
    let fixture = selector_scope_fixture()?;
    if staged {
        run_git(&fixture.root, &["add", "src"])?;
    }
    let root = fixture.root.to_string_lossy().into_owned();
    let mut errors = Vec::new();
    for (label, flags, expected) in [
        ("dirty default", vec!["--base", "HEAD~1"], &fixture.worktree),
        (
            "worktree",
            vec!["--base", "HEAD~1", "--worktree"],
            &fixture.worktree,
        ),
        (
            "supplied",
            vec!["--diff", &fixture.patch],
            &fixture.committed,
        ),
        (
            "committed",
            vec!["--base", "HEAD~1", "--committed"],
            &fixture.committed,
        ),
    ] {
        let artifact = fixture.root.join(format!("{label}.artifact.json"));
        let artifact = artifact.to_string_lossy().into_owned();
        let mut args = vec!["check", "--root", &root];
        args.extend_from_slice(&flags);
        let check = run_ripr(&args);
        assert_success(&check);
        let human = String::from_utf8_lossy(&check.stdout);
        let commands: Vec<_> = human
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("ripr explain ") || line.starts_with("ripr context "))
            .collect();
        if commands.len() != 2 {
            return Err(format!(
                "{label} must produce exactly two drill-ins: {human}"
            ));
        }
        for command in commands {
            let args = printed_ripr_args(command)?;
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let output = run_ripr(&args);
            if !output.status.success()
                || !String::from_utf8_lossy(&output.stdout).contains(expected)
            {
                errors.push(format!("staged={staged} {label}: `{command}` returned {:?}, expected {expected}; stderr={}", output.status.code(), String::from_utf8_lossy(&output.stderr)));
            } else if args[0] == "context" {
                let packet: serde_json::Value = serde_json::from_slice(&output.stdout)
                    .map_err(|err| format!("context JSON: {err}"))?;
                let reciprocal = packet
                    .pointer("/witness/explain_command")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        format!("{label} context has no reciprocal explain: {packet}")
                    })?;
                let reciprocal_args = printed_ripr_args(reciprocal)?;
                let reciprocal_args: Vec<&str> =
                    reciprocal_args.iter().map(String::as_str).collect();
                let replay = run_ripr(&reciprocal_args);
                if !replay.status.success()
                    || !String::from_utf8_lossy(&replay.stdout).contains(expected)
                {
                    errors.push(format!(
                        "{label} reciprocal explain lost {expected}: {replay:?}"
                    ));
                }
            }
        }
        args.extend_from_slice(&["--write-artifact", &artifact, "--json"]);
        let recorded = run_ripr(&args);
        assert_success(&recorded);
        for command in ["explain", "context"] {
            let mut args = vec![command, "--root", &root, "--from", &artifact];
            if command == "context" {
                args.push("--at");
            }
            args.push(expected);
            let replay = run_ripr(&args);
            if !replay.status.success()
                || !String::from_utf8_lossy(&replay.stdout).contains(expected)
            {
                return Err(format!(
                    "{label} artifact {command} must preserve {expected}: {replay:?}"
                ));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    ignore_remove_dir_all(&fixture.root);
    Ok(())
}

#[test]
fn committed_check_printed_drill_ins_preserve_unstaged_scope() -> Result<(), String> {
    printed_drill_in_scope(false)
}

#[test]
fn committed_check_printed_drill_ins_preserve_staged_scope() -> Result<(), String> {
    printed_drill_in_scope(true)
}

#[test]
fn committed_drill_in_scope_conflicts_refuse_before_input_reads() -> Result<(), String> {
    let absent = unique_temp_workspace("committed-conflict-missing-root");
    let absent = absent.to_string_lossy().into_owned();
    for command in ["explain", "context"] {
        for (conflict, other) in [
            ("--worktree", vec!["--worktree"]),
            ("--diff", vec!["--diff", "-"]),
            ("--from", vec!["--from", "missing.artifact.json"]),
        ] {
            for committed_first in [false, true] {
                let mut args = vec![command, "--root", &absent];
                if committed_first {
                    args.push("--committed");
                }
                args.extend_from_slice(&other);
                if !committed_first {
                    args.push("--committed");
                }
                if command == "context" {
                    args.push("--at");
                }
                args.push("probe:missing");
                let output = run_ripr(&args);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if output.status.code() != Some(2)
                    || !output.stdout.is_empty()
                    || !stderr.contains("--committed")
                    || !stderr.contains(conflict)
                    || stderr.contains("loading_input")
                    || stderr.contains("does not exist")
                {
                    return Err(format!(
                        "{command} {conflict} must refuse before reading root/stdin/artifact: {output:?}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// A manual committed drill-in from a nested cwd resolves the same root as
/// check/explain, even when the worktree contains a different finding.
#[test]
fn committed_context_from_nested_cwd_selects_the_check_finding() -> Result<(), String> {
    let fixture = selector_scope_fixture()?;
    let nested = fixture.root.join("src");
    let run = |args: &[&str]| {
        run_command(env!("CARGO_BIN_EXE_ripr"), Some(&nested), args)
            .map_err(|err| format!("nested CLI: {err}"))
    };
    let check = run(&["check", "--base", "HEAD~1", "--committed", "--json"])?;
    assert_success(&check);
    assert_eq!(first_finding_id(&check.stdout)?, fixture.committed);
    let root = fixture.root.to_string_lossy().into_owned();
    for explicit_root in [false, true] {
        for command in ["explain", "context"] {
            let mut args = vec![command, "--base", "HEAD~1", "--committed"];
            if explicit_root {
                args.extend_from_slice(&["--root", &root]);
            }
            if command == "context" {
                args.push("--at");
            }
            args.push(&fixture.committed);
            let selected = run(&args)?;
            assert_success(&selected);
            if command == "context" {
                let packet: serde_json::Value = serde_json::from_slice(&selected.stdout)
                    .map_err(|err| format!("nested context JSON: {err}"))?;
                assert_eq!(packet["probe"]["id"], fixture.committed);
            } else {
                assert!(String::from_utf8_lossy(&selected.stdout).contains(&fixture.committed));
            }
        }
    }
    // Explicit `--root .` opts out of discovery and keeps the nested scope.
    let narrow = run(&[
        "check",
        "--root",
        ".",
        "--base",
        "HEAD~1",
        "--committed",
        "--json",
    ])?;
    assert_success(&narrow);
    assert!(!String::from_utf8_lossy(&narrow.stdout).contains(&fixture.committed));
    let missed = run(&[
        "context",
        "--root",
        ".",
        "--base",
        "HEAD~1",
        "--committed",
        "--at",
        &fixture.committed,
    ])?;
    assert_eq!(missed.status.code(), Some(2));
    assert!(missed.stdout.is_empty());
    ignore_remove_dir_all(&fixture.root);
    Ok(())
}

fn composed_recovery_choices(staged: bool) -> Result<(), String> {
    let fixture = selector_scope_fixture()?;
    if staged {
        run_git(&fixture.root, &["add", "src"])?;
    }
    let nested = fixture.root.join("src");
    let run = |args: &[&str]| {
        run_command(env!("CARGO_BIN_EXE_ripr"), Some(&nested), args)
            .map_err(|err| format!("nested recovery CLI: {err}"))
    };
    for command in ["explain", "context"] {
        // Recovery preference must not change the dirty-default analysis.
        let mut hit = vec![command, "--base", "HEAD~1"];
        if command == "context" {
            hit.push("--at");
        }
        hit.push(&fixture.worktree);
        let selected = run(&hit)?;
        assert_success(&selected);
        assert!(String::from_utf8_lossy(&selected.stdout).contains(&fixture.worktree));

        for (label, scope, missing, expected, excluded, listings) in [
            (
                "dirty default",
                vec!["--base", "HEAD~1"],
                &fixture.committed,
                &fixture.committed,
                &fixture.worktree,
                2,
            ),
            (
                "explicit worktree",
                vec!["--base", "HEAD~1", "--worktree"],
                &fixture.committed,
                &fixture.worktree,
                &fixture.committed,
                1,
            ),
            (
                "explicit committed",
                vec!["--base", "HEAD~1", "--committed"],
                &fixture.worktree,
                &fixture.committed,
                &fixture.worktree,
                2,
            ),
            (
                "supplied diff",
                vec!["--diff", &fixture.patch],
                &fixture.worktree,
                &fixture.committed,
                &fixture.worktree,
                1,
            ),
        ] {
            let mut args = vec![command];
            args.extend_from_slice(&scope);
            if command == "context" {
                args.push("--at");
            }
            args.push(missing);
            let miss = run(&args)?;
            assert_eq!(miss.status.code(), Some(2), "{label}: {miss:?}");
            assert!(miss.stdout.is_empty(), "{label}: {miss:?}");
            let stderr = String::from_utf8_lossy(&miss.stderr);
            let emitted: Vec<_> = stderr
                .split('`')
                .filter(|text| text.starts_with("ripr check "))
                .collect();
            assert_eq!(emitted.len(), listings, "{label}: {stderr}");
            if label == "dirty default" {
                assert!(stderr.contains("working-tree scope was analyzed and also missed"));
            }
            for (index, printed) in emitted.iter().enumerate() {
                let args = printed_ripr_args(printed)?;
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let replay = run(&args)?;
                assert_success(&replay);
                let (expected, excluded) = if index == 0 {
                    (expected, excluded)
                } else {
                    (&fixture.worktree, &fixture.committed)
                };
                let report: serde_json::Value = serde_json::from_slice(&replay.stdout)
                    .map_err(|err| format!("recovery listing JSON: {err}"))?;
                let findings = report["findings"].as_array().ok_or("missing findings")?;
                assert!(findings.iter().any(|f| f["id"] == expected.as_str()));
                assert!(!findings.iter().any(|f| f["id"] == excluded.as_str()));
                if label != "supplied diff" {
                    let worktree = index > 0 || label == "explicit worktree";
                    assert_eq!(args.contains(&"--worktree"), worktree);
                    assert_eq!(args.contains(&"--committed"), !worktree);
                }
            }
        }
    }
    ignore_remove_dir_all(&fixture.root);
    Ok(())
}

#[test]
fn composed_recovery_preserves_unstaged_analysis_and_explicit_sources() -> Result<(), String> {
    composed_recovery_choices(false)
}

#[test]
fn composed_recovery_preserves_staged_analysis_and_explicit_sources() -> Result<(), String> {
    composed_recovery_choices(true)
}

use super::*;
use std::cell::{Cell, RefCell};

#[test]
fn shared_leaves_run_once_and_fast_only_leaves_still_execute() -> Result<(), String> {
    let counts = RefCell::new(BTreeMap::new());
    let execute = |name: &str| {
        *counts.borrow_mut().entry(name.to_string()).or_insert(0) += 1;
        Ok(())
    };
    transaction(
        || Ok(0),
        || {
            for name in SHARED {
                execute(name)?;
            }
            Ok(())
        },
        |proof| {
            for name in SHARED {
                leaf(Some(proof), name, || execute(name))?;
            }
            for name in ["clippy", "check-support-tiers", "cli-help-parity"] {
                leaf(Some(proof), name, || execute(name))?;
            }
            Ok(())
        },
    )?;
    assert_eq!(counts.borrow().len(), SHARED.len() + 3);
    assert!(counts.borrow().values().all(|n| *n == 1));
    Ok(())
}

#[test]
fn missing_proof_executes_the_original_leaf_and_propagates_failure() {
    let ran = Cell::new(false);
    let result = leaf(None, "check-file-policy", || {
        ran.set(true);
        Err("native failure".into())
    });
    assert!(ran.get());
    assert_eq!(result, Err("native failure".into()));
}

#[test]
fn policy_failure_never_issues_shared_proof_or_runs_fast() {
    let ran = Cell::new(false);
    let result = transaction(
        || Ok(0),
        || Err("policy refused".into()),
        |_| {
            ran.set(true);
            Ok(())
        },
    );
    assert_eq!(result, Err("policy refused".into()));
    assert!(!ran.get());
}

#[test]
fn phase_and_context_failures_cannot_be_success() {
    let fast = transaction(|| Ok(0), || Ok(()), |_| Err("clippy refused".into()));
    assert_eq!(fast, Err("clippy refused".into()));
    for phase in [2, 3] {
        let calls = Cell::new(0);
        let fast_ran = Cell::new(false);
        let result = transaction(
            || {
                let n = calls.get() + 1;
                calls.set(n);
                Ok(n >= phase)
            },
            || Ok(()),
            |_| {
                fast_ran.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fast_ran.get(), phase == 3);
    }
    let missing = transaction(
        || Err::<u8, _>("context unavailable".into()),
        || Ok(()),
        |_| Ok(()),
    );
    assert!(missing.is_err());
}

#[test]
fn disclosure_distinguishes_shared_work_from_fast_execution() -> Result<(), String> {
    transaction(
        || Ok(0),
        || Ok(()),
        |proof| {
            let disclosure = reuse_disclosure(Some(proof), &["check-file-policy", "clippy"]);
            assert!(disclosure.contains("- check-file-policy"));
            assert!(!disclosure.contains("- clippy"));
            assert!(reuse_disclosure(None, &["check-file-policy"]).is_empty());
            Ok(())
        },
    )
}
#[test]
fn every_context_dimension_refuses_reuse_on_change() {
    let context = Context {
        cwd: PathBuf::from("source"),
        selector: crate::check_fast_selector::Selection {
            base: "a".repeat(40),
            head: "b".repeat(40),
            merge_base: "c".repeat(40),
            files: vec!["x.rs".into()],
        },
        tree: vec![1],
        environment: BTreeMap::new(),
        source: vec![1],
        executable: vec![1],
        index: vec![1],
        git_config: vec![1],
        tools: vec![vec![1]],
        cargo_config: vec![vec![1]],
    };
    let mut mutations = Vec::new();
    let mut changed = context.clone();
    changed.cwd.push("other");
    mutations.push(changed);
    let mut changed = context.clone();
    changed.selector.base = "d".repeat(40);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.selector.head = "d".repeat(40);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.selector.merge_base = "d".repeat(40);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.selector.files.push("extra.rs".into());
    mutations.push(changed);
    let mut changed = context.clone();
    changed
        .environment
        .insert("RUSTFLAGS".into(), "different".into());
    mutations.push(changed);
    let mut changed = context.clone();
    changed.source.push(2);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.executable.push(2);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.index.push(2);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.git_config.push(2);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.tools.push(vec![2]);
    mutations.push(changed);
    let mut changed = context.clone();
    changed.cargo_config.push(vec![2]);
    mutations.push(changed);
    for mutation in mutations {
        for phase in [2, 3] {
            let calls = Cell::new(0);
            let result = transaction(
                || {
                    calls.set(calls.get() + 1);
                    Ok(if calls.get() < phase {
                        context.clone()
                    } else {
                        mutation.clone()
                    })
                },
                || Ok(()),
                |_| Ok(()),
            );
            assert!(
                result.is_err(),
                "context mutation accepted at phase {phase}"
            );
        }
    }
}

#[test]
fn source_bytes_are_inputs_even_without_git_or_timestamp_changes() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "session-source-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let result = (|| {
        fs::write(root.join("source.rs"), "first").map_err(|e| e.to_string())?;
        let before = source_digest(&root, b"source.rs\0")?;
        fs::write(root.join("source.rs"), "other").map_err(|e| e.to_string())?;
        assert_ne!(before, source_digest(&root, b"source.rs\0")?);
        if let Ok(digest) = source_digest(&root, b"missing.rs\0") {
            return Err(format!("missing source produced a digest: {digest:?}"));
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(root).map_err(|e| e.to_string());
    result.and(cleanup)
}

#[test]
fn native_snapshot_refuses_dirty_source_and_observes_ref_and_config_changes() -> Result<(), String>
{
    let root = std::env::temp_dir().join(format!(
        "session-context-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| output_at("git", args, &root);
    let capture = || Context::capture_in(root.clone());
    let result = (|| {
        git(&["init", "--initial-branch=main"])?;
        git(&["config", "user.name", "Session fixture"])?;
        git(&["config", "user.email", "session@example.invalid"])?;
        fs::write(root.join("source.rs"), "first").map_err(|e| e.to_string())?;
        fs::write(root.join(".gitignore"), "target/\n.cargo/\n*.hidden\n")
            .map_err(|e| e.to_string())?;
        git(&["add", "."])?;
        git(&["-c", "commit.gpgsign=false", "commit", "-m", "base"])?;
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"])?;
        let before = capture()?;
        assert!(capture()? == before);
        fs::create_dir_all(root.join("target")).map_err(|e| e.to_string())?;
        fs::write(root.join("target/report"), "new generated output").map_err(|e| e.to_string())?;
        assert!(
            capture()? == before,
            "ignored build/report output is not source"
        );
        fs::write(root.join("source.rs"), "other").map_err(|e| e.to_string())?;
        assert!(capture().is_err(), "dirty source accepted");
        git(&["add", "source.rs"])?;
        assert!(capture().is_err(), "changed index accepted");
        git(&["-c", "commit.gpgsign=false", "commit", "-m", "successor"])?;
        assert!(capture()? != before, "changed HEAD accepted");
        let successor = capture()?;
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"])?;
        assert!(capture()? != successor, "changed base accepted");
        let rebased = capture()?;
        #[cfg(unix)]
        {
            fs::create_dir_all(root.join("alias")).map_err(|e| e.to_string())?;
            fs::write(root.join("alias/input.hidden"), "first").map_err(|e| e.to_string())?;
            fs::write(root.join("alias\\input.hidden"), "first").map_err(|e| e.to_string())?;
            let aliases = capture()?;
            fs::write(root.join("alias\\input.hidden"), "other").map_err(|e| e.to_string())?;
            assert!(
                capture()? != aliases,
                "literal backslash path collapsed into a different source"
            );
            fs::remove_file(root.join("alias\\input.hidden")).map_err(|e| e.to_string())?;
            fs::remove_dir_all(root.join("alias")).map_err(|e| e.to_string())?;
        }

        fs::write(root.join("ignored.hidden"), "live policy input").map_err(|e| e.to_string())?;
        assert!(
            capture()? != rebased,
            "ignored live policy input change accepted"
        );
        fs::remove_file(root.join("ignored.hidden")).map_err(|e| e.to_string())?;

        fs::create_dir_all(root.join(".cargo")).map_err(|e| e.to_string())?;
        fs::write(root.join(".cargo/config.toml"), "[build]\njobs = 1\n")
            .map_err(|e| e.to_string())?;
        assert!(
            capture()? != rebased,
            "ignored Cargo config change accepted"
        );
        fs::write(root.join("untracked.rs"), "extra").map_err(|e| e.to_string())?;
        assert!(capture().is_err(), "untracked source accepted");
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(root).map_err(|e| e.to_string());
    result.and(cleanup)
}

#[test]
fn shared_ledger_is_bound_to_the_native_policy_and_fast_callers() {
    let main = include_str!("../main.rs");
    let all = include_str!("../policy/all.rs");
    let dispatch = include_str!("../dispatch.rs");
    let policy = main
        .split("fn precommit_policy(collect_all: bool)")
        .nth(1)
        .and_then(|body| body.split("/// Output-surface diffs").next())
        .unwrap_or("");
    let fast = main
        .split("fn check_fast_gates(")
        .nth(1)
        .and_then(|body| body.split("fn check_fast_diagnostics").next())
        .unwrap_or("");
    assert!(dispatch.contains(
        "crate::session_check::run(precommit_v2::run_policy, check_fast_strict::run_shared)"
    ));
    for name in SHARED {
        assert!(
            fast.contains(&format!("session_check::leaf(proof, \"{name}\"")),
            "fast reuse caller missing: {name}"
        );
        match *name {
            "fmt --check" => assert!(policy.contains("run(\"cargo\", &[\"fmt\", \"--check\"])?;")),
            "check-workflows" => assert!(policy.contains("None => check_workflows()?,")),
            "check-file-policy" => {
                assert!(policy.contains("check_file_policy_deferring(&mut deferred)?;"));
                assert!(policy.contains("match check_file_policy()"));
            }
            _ => {
                let call = format!("{}()", name.replace('-', "_"));
                assert!(
                    all.contains(&call) || policy.contains(&format!("{call}?;")),
                    "shared native policy owner missing: {name}"
                );
            }
        }
    }
}

//! #14704: child-only injection proves scenario isolation without global env writes.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

const CHILD: &str = "reports::agent_eval::isolation_tests::isolated_scenario_child";

fn raw_git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let removed: Vec<String> = std::env::vars_os()
        .filter_map(|(key, _)| key.into_string().ok())
        .filter(|key| key.to_ascii_uppercase().starts_with("GIT_"))
        .collect();
    let removed: Vec<&str> = removed.iter().map(String::as_str).collect();
    let mut envs = GIT_IDENTITY.to_vec();
    envs.extend([
        ("GIT_CONFIG_NOSYSTEM", "1"),
        (
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        ),
    ]);
    let output = capture_bytes_in_dir_with_input_env_budget(
        Path::new("git"),
        &strings(args),
        (root, Some(&[])),
        (&envs, &removed),
        ByteCaptureBudget {
            timeout: STEP_TIMEOUT,
            stdout_bytes: 1024 * 1024,
            stderr_bytes: 1024 * 1024,
        },
        "disposable caller setup/inspection",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "caller git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

fn snapshot(caller: &Path) -> Result<Vec<Option<Vec<u8>>>, String> {
    let read = |path: &str| match fs::read(caller.join(path)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    };
    Ok(vec![
        Some(raw_git(caller, &["rev-parse", "HEAD"])?),
        Some(raw_git(caller, &["show-ref"])?),
        read(".git/HEAD")?,
        read(".git/index")?,
        read("tracked")?,
    ])
}

#[test]
fn scenario_children_ignore_caller_git_environment() -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "agent-eval-isolation-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let caller = root.join("caller");
    fs::create_dir(&caller).map_err(|e| e.to_string())?;
    raw_git(&caller, &["init", "-q", "-b", "main"])?;
    write_file(&caller.join("tracked"), "committed\n")?;
    raw_git(&caller, &["add", "tracked"])?;
    raw_git(
        &caller,
        &[
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=unused",
            "commit",
            "-qm",
            "caller",
        ],
    )?;
    write_file(&caller.join("tracked"), "staged\n")?;
    raw_git(&caller, &["add", "tracked"])?;
    write_file(&caller.join("tracked"), "unstaged\n")?;
    let git_dir = caller.join(".git").display().to_string();
    let worktree = caller.display().to_string();
    let index = caller.join(".git/index").display().to_string();
    let objects = caller.join(".git/objects").display().to_string();
    let config = root.join("injected.config");
    write_file(
        &config,
        &format!("[core]\nworktree = {worktree}\n[alias]\nstatus = !exit 99\n"),
    )?;
    let config_path = config.display().to_string();
    let home = root.display().to_string();
    let xdg = root.join("xdg").display().to_string();
    let attributes = root.join("injected.attributes");
    write_file(&attributes, "*.rs filter=isolation-test\n")?;
    let body = format!(
        "[core]\nattributesFile = {:?}\n[filter \"isolation-test\"]\nclean = git agent-eval-missing-filter\nrequired = true\n",
        attributes.to_string_lossy()
    );
    write_file(&root.join(".gitconfig"), &body)?;
    write_file(&root.join("xdg/git/config"), &body)?;
    let parameters = format!("'core.worktree={worktree}'");
    let cases = [
        vec![],
        vec![("GIT_DIR", git_dir.as_str())],
        vec![("GIT_WORK_TREE", worktree.as_str())],
        vec![("GIT_INDEX_FILE", index.as_str())],
        vec![("GIT_COMMON_DIR", git_dir.as_str())],
        vec![("GIT_OBJECT_DIRECTORY", objects.as_str())],
        vec![("GIT_ALTERNATE_OBJECT_DIRECTORIES", objects.as_str())],
        vec![
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "core.worktree"),
            ("GIT_CONFIG_VALUE_0", worktree.as_str()),
        ],
        vec![("GIT_CONFIG_PARAMETERS", parameters.as_str())],
        vec![("HOME", home.as_str()), ("XDG_CONFIG_HOME", xdg.as_str())],
        vec![("GIT_CONFIG_GLOBAL", config_path.as_str())],
        vec![
            ("GIT_CONFIG", config_path.as_str()),
            ("GIT_CONFIG_SYSTEM", config_path.as_str()),
        ],
        vec![
            ("GIT_DIR", git_dir.as_str()),
            ("GIT_WORK_TREE", worktree.as_str()),
            ("GIT_INDEX_FILE", index.as_str()),
            ("GIT_COMMON_DIR", git_dir.as_str()),
            ("GIT_OBJECT_DIRECTORY", objects.as_str()),
            ("GIT_ALTERNATE_OBJECT_DIRECTORIES", objects.as_str()),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "core.worktree"),
            ("GIT_CONFIG_VALUE_0", worktree.as_str()),
        ],
        #[cfg(windows)]
        vec![("git_dir", git_dir.as_str())],
    ];
    let inherited: Vec<String> = std::env::vars_os()
        .filter_map(|(key, _)| key.into_string().ok())
        .filter(|key| key.to_ascii_uppercase().starts_with("GIT_"))
        .collect();
    let removed: Vec<&str> = inherited.iter().map(String::as_str).collect();
    let before = snapshot(&caller)?;
    for (case, poison) in cases.iter().enumerate() {
        let scratch = root.join(format!("scenario-{case}"));
        let scratch_text = scratch.display().to_string();
        let mut envs = poison.clone();
        envs.push(("AGENT_EVAL_ISOLATION_CHILD", &scratch_text));
        envs.push(("AGENT_EVAL_CALLER", &worktree));
        let output = capture_bytes_in_dir_with_input_env_budget(
            &std::env::current_exe().map_err(|e| e.to_string())?,
            &strings(&["--exact", CHILD, "--nocapture"]),
            (&root, Some(&[])),
            (&envs, &removed),
            ByteCaptureBudget {
                timeout: STEP_TIMEOUT,
                stdout_bytes: 1024 * 1024,
                stderr_bytes: 1024 * 1024,
            },
            "agent-eval isolation child",
        )?;
        // Check integrity even when the child failed. Keep failure artifacts.
        assert_eq!(
            snapshot(&caller)?,
            before,
            "caller changed in case {case}; retained {}",
            root.display()
        );
        assert!(!output.timed_out);
        assert!(
            output.status.is_some_and(|status| status.success()),
            "case {case}; retained {}: {}{}",
            root.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("SCENARIO_DIRECT_AND_ANALYZER_ISOLATED"),
            "child did not exercise the scenario: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "isolation case {case} {:?}: direct Git and analyzer CLI success/failure passed; caller HEAD/refs/index/tracked bytes unchanged; scratch commits independently readable",
            poison.iter().map(|(name, _)| *name).collect::<Vec<_>>()
        );
    }
    fs::remove_dir_all(&root).map_err(|e| e.to_string())
}

#[test]
fn isolated_scenario_child() -> Result<(), String> {
    let Some(scratch) = std::env::var_os("AGENT_EVAL_ISOLATION_CHILD") else {
        return Ok(());
    };
    let caller = PathBuf::from(std::env::var_os("AGENT_EVAL_CALLER").ok_or("missing caller path")?);
    let before = snapshot(&caller)?;
    let workspace = materialize(Path::new(&scratch), &SCENARIOS[0], "direct")?;
    assert_eq!(
        snapshot(&caller)?,
        before,
        "caller changed after materialize"
    );
    assert!(workspace.root.join(".git/objects").is_dir());
    assert_eq!(
        String::from_utf8(raw_git(&workspace.root, &["log", "--format=%s", "--all"])?)
            .map_err(|e| e.to_string())?
            .lines()
            .collect::<Vec<_>>(),
        vec!["change", "base"]
    );
    let output = capture(
        Path::new("git"),
        &strings(&["rev-parse", "--show-toplevel"]),
        &workspace.root,
        None,
        &[],
    )?;
    assert_eq!(output.exit, Some(0), "{}", output.stderr);
    assert_eq!(
        fs::canonicalize(output.stdout.trim()).map_err(|e| e.to_string())?,
        fs::canonicalize(&workspace.root).map_err(|e| e.to_string())?
    );
    let commits = capture(
        Path::new("git"),
        &strings(&["log", "--format=%s", "--all"]),
        &workspace.root,
        None,
        &[],
    )?;
    assert_eq!(commits.exit, Some(0));
    assert_eq!(
        commits.stdout.lines().collect::<Vec<_>>(),
        vec!["change", "base"]
    );
    let caller_head =
        String::from_utf8(raw_git(&caller, &["rev-parse", "HEAD"])?).map_err(|e| e.to_string())?;
    let leaked = capture(
        Path::new("git"),
        &strings(&["cat-file", "-e", caller_head.trim()]),
        &workspace.root,
        None,
        &[],
    )?;
    assert!(
        matches!(leaked.exit, Some(1 | 128)),
        "scenario can read the caller-only commit, or cat-file ended without the expected rejection: {:?}",
        leaked.exit
    );
    let failure = git(
        &workspace.root,
        &["checkout", "missing-ref-for-isolation-control"],
    )
    .err()
    .ok_or("missing checkout unexpectedly succeeded")?;
    assert!(failure.contains("missing-ref-for-isolation-control"));
    assert_eq!(
        snapshot(&caller)?,
        before,
        "caller changed after Git failure"
    );
    for mode in ["success", "failure"] {
        let analyzer = capture(
            &std::env::current_exe().map_err(|e| e.to_string())?,
            &strings(&[
                "--exact",
                "reports::agent_eval::isolation_tests::analyzer_descendant_child",
                "--nocapture",
            ]),
            &workspace.root,
            None,
            &[("AGENT_EVAL_ANALYZER_CHILD", mode)],
        )?;
        assert_eq!(
            snapshot(&caller)?,
            before,
            "caller changed after analyzer {mode}"
        );
        assert_eq!(
            analyzer.exit,
            Some(0),
            "{}{}",
            analyzer.stdout,
            analyzer.stderr
        );
        if mode == "success" {
            assert!(analyzer.stdout.contains("ANALYZER_SCENARIO_OK"));
            assert!(
                analyzer.stdout.contains("shipping_cents"),
                "{}",
                analyzer.stdout
            );
            assert!(
                analyzer.stdout.contains("src/lib.rs:5"),
                "{}",
                analyzer.stdout
            );
        } else {
            assert!(analyzer.stdout.contains("ANALYZER_EXPECTED_FAILURE"));
        }
    }
    // Repository discovery still belongs to the scenario after a failed step.
    let after = capture(
        Path::new("git"),
        &strings(&["rev-parse", "--show-toplevel"]),
        &workspace.root,
        None,
        &[],
    )?;
    assert_eq!(after.stdout, output.stdout);
    assert_eq!(after.exit, Some(0));
    println!("SCENARIO_DIRECT_AND_ANALYZER_ISOLATED");
    Ok(())
}

#[test]
fn analyzer_descendant_child() -> Result<(), String> {
    let Ok(mode) = std::env::var("AGENT_EVAL_ANALYZER_CHILD") else {
        return Ok(());
    };
    let base = if mode == "success" {
        "main"
    } else {
        "missing-isolation-base"
    };
    let result = ripr::cli::run(strings(&[
        "ripr", "check", "--quiet", "--format", "agent", "--base", base,
    ]));
    if mode == "success" {
        result.map_err(|e| e.to_string())?;
        println!("ANALYZER_SCENARIO_OK");
    } else {
        let error = result.err().ok_or("missing scenario base must fail")?;
        assert!(
            error.to_string().contains("missing-isolation-base"),
            "{error}"
        );
        println!("ANALYZER_EXPECTED_FAILURE");
    }
    Ok(())
}

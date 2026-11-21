//! A marker must not replace the standalone project's actual analysis context.
use super::{
    DeclaredHunkScratch, run_command, run_command_with_env, run_git, unique_external_workspace,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[cfg(unix)]
#[test]
fn broken_git_links_keep_implicit_check_and_cache_inside_the_project() -> TestResult {
    for (label, target) in [("dangling", "missing-gitdir"), ("cycle", ".git")] {
        let outer = unique_external_workspace(label)?;
        let _cleanup = DeclaredHunkScratch(outer.clone());
        let project = outer.join("project");
        let cwd = project.join("src");
        fs::create_dir_all(&cwd)?;
        fs::create_dir_all(project.join("tests"))?;
        fs::write(
            outer.join("Cargo.toml"),
            "[workspace]\nmembers = [\"project\"]\n",
        )?;
        fs::write(outer.join("ripr.toml"), "[analysis]\nmode = \"deep\"\n")?;
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        fs::write(cwd.join("lib.rs"), "pub fn price() -> u32 { 2 }\n")?;
        fs::write(
            project.join("tests/price.rs"),
            "use pricing::price;\n#[test]\nfn exact_price() { assert_eq!(price(), 2); }\n",
        )?;
        let diff = project.join("change.diff");
        fs::write(
            &diff,
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn price() -> u32 { 1 }\n+pub fn price() -> u32 { 2 }\n",
        )?;
        std::os::unix::fs::symlink(target, project.join(".git"))?;
        let project_text = project.to_str().ok_or("project path must be UTF-8")?;
        let diff_text = diff.to_str().ok_or("diff path must be UTF-8")?;
        let check = |extra: &[&str], mode: &str| -> TestResult<Value> {
            let mut args = vec!["check", "--diff", diff_text, "--format", "json"];
            args.extend_from_slice(extra);
            let output = run_command_with_env(
                env!("CARGO_BIN_EXE_ripr"),
                &cwd,
                &args,
                &[("RIPR_CACHE_DIR", "")],
            )?;
            assert!(
                output.status.success(),
                "{label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(report["mode"], mode, "{label}: enclosing policy leaked");
            assert_eq!(report["summary"]["probes"], 1, "{label}: nonempty analysis");
            assert_eq!(
                report["summary"]["findings"], 1,
                "{label}: nonempty analysis"
            );
            assert_eq!(
                report["root"].as_str().map(Path::new),
                Some(project.as_path()),
                "{label}: analysis root escaped"
            );
            Ok(report)
        };
        let explicit = check(&["--root", project_text], "draft")?;
        let implicit = check(&[], "draft")?;
        assert_eq!(implicit["findings"], explicit["findings"]);
        let cache = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &cwd,
            &["cache", "status", "--json"],
            &[("RIPR_CACHE_DIR", "")],
        )?;
        assert!(
            cache.status.success(),
            "{}",
            String::from_utf8_lossy(&cache.stderr)
        );
        let cache: Value = serde_json::from_slice(&cache.stdout)?;
        assert_eq!(
            cache["cache_dir"].as_str().map(Path::new),
            Some(project.join("target/ripr/cache").as_path())
        );

        fs::write(project.join("ripr.toml"), "[analysis]\nmode = \"ready\"\n")?;
        check(&[], "ready")?;
        check(&["--mode", "draft"], "draft")?;
        check(&["--root", project_text, "--mode", "instant"], "instant")?;
        fs::remove_dir_all(outer)?;
    }
    Ok(())
}

fn python_fixture(label: &str) -> TestResult<(PathBuf, PathBuf, DeclaredHunkScratch)> {
    let sandbox = unique_external_workspace(label)?;
    let cleanup = DeclaredHunkScratch(sandbox.clone());
    let project = sandbox.join("project");
    fs::create_dir_all(project.join("src"))?;
    fs::create_dir_all(project.join("tests"))?;
    // Cargo points TMPDIR into this checkout. This fixture must instead be
    // outside its real Git ancestor before adding deliberately inert metadata.
    let outside = run_command_with_env(
        "git",
        &sandbox,
        &["rev-parse", "--show-toplevel"],
        &[("LC_ALL", "C")],
    )?;
    assert_eq!(outside.status.code(), Some(128));
    assert!(
        String::from_utf8_lossy(&outside.stderr).contains("not a git repository"),
        "fixture must lack an ambient Git ancestor before setup: {}",
        String::from_utf8_lossy(&outside.stderr)
    );
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"discount\"\nversion = \"0.1.0\"\n",
    )?;
    fs::write(
        project.join("src/discount.py"),
        "def discount(amount):\n    if amount >= 100:\n        return 10\n    return 0\n",
    )?;
    fs::write(
        project.join("tests/test_discount.py"),
        "from src.discount import discount\n\ndef test_below():\n    assert discount(99) == 0\n\ndef test_above():\n    assert discount(101) == 10\n",
    )?;
    fs::write(
        project.join("change.diff"),
        "--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,4 +1,4 @@\n def discount(amount):\n-    if amount > 100:\n+    if amount >= 100:\n         return 10\n     return 0\n",
    )?;
    Ok((sandbox, project, cleanup))
}

fn check(project: &Path, explicit: bool, env: &[(&str, &str)]) -> TestResult<Value> {
    let mut args = vec![
        "check",
        "--diff",
        "change.diff",
        "--mode",
        "fast",
        "--format",
        "json",
    ];
    if explicit {
        args.extend(["--root", "."]);
    }
    let output = run_command_with_env(env!("CARGO_BIN_EXE_ripr"), project, &args, env)?;
    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("resolved workspace root to"),
        "a standalone project must retain its root: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["root"], ".");
    let outcome = &report["analysis_outcome"]["outcome"];
    assert_eq!(outcome["counts"]["changed_file_count"], 1);
    assert_eq!(outcome["counts"]["changed_line_count"], 2);
    assert_eq!(report["summary"]["probes"], 1);
    assert_eq!(report["summary"]["findings"], 1);
    assert_eq!(report["summary"]["weakly_exposed"], 1);
    assert_eq!(report["findings"][0]["related_tests_total"], 2);
    assert_eq!(
        report["findings"][0]["related_tests"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(report["preview_languages"][0]["enabled"], true);
    assert_eq!(report["preview_languages"][0]["analyzed"], true);
    assert_eq!(outcome["kind"], "complete_with_findings");
    assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
    assert_eq!(outcome["limitations"], serde_json::json!([]));
    assert!(
        report["findings"][0]["evidence"]
            .as_array()
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item == "missing_discriminator: amount == 100")
            })
    );
    Ok(report)
}

#[test]
fn invalid_ancestor_git_markers_preserve_python_and_cache_context() -> TestResult {
    for marker in ["empty-directory", "invalid-file", "missing-gitdir"] {
        let (sandbox, project, _sandbox_guard) = python_fixture(marker)?;
        match marker {
            "empty-directory" => fs::create_dir(sandbox.join(".git"))?,
            "invalid-file" => fs::write(sandbox.join(".git"), "leftover metadata\n")?,
            _ => fs::write(sandbox.join(".git"), "gitdir: missing\n")?,
        }
        // Setup must establish that Git itself refuses the marker.
        let git = run_command(
            "git",
            Some(&sandbox),
            &["--git-dir=.git", "rev-parse", "--show-toplevel"],
        )?;
        assert!(
            !git.status.success(),
            "inert marker unexpectedly became a repository"
        );
        let explicit = check(&project, true, &[])?;
        let implicit = check(&project, false, &[])?;
        assert_eq!(implicit["findings"], explicit["findings"]);
        assert_eq!(implicit["analysis_outcome"], explicit["analysis_outcome"]);

        let cache = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &project,
            &["cache", "status", "--json"],
            &[("RIPR_CACHE_DIR", "")],
        )?;
        assert!(
            cache.status.success(),
            "{}",
            String::from_utf8_lossy(&cache.stderr)
        );
        let cache: Value = serde_json::from_slice(&cache.stdout)?;
        assert_eq!(
            cache["cache_dir"].as_str().map(Path::new),
            Some(project.canonicalize()?.join("target/ripr/cache").as_path())
        );

        // The positive is an actual repository, with the exact same sources/diff.
        run_git(&project, &["init", "-b", "main"])?;
        let real_git = check(&project, false, &[])?;
        assert_eq!(real_git["findings"], explicit["findings"]);
        assert_eq!(real_git["analysis_outcome"], explicit["analysis_outcome"]);
        fs::remove_dir_all(sandbox)?;
    }
    Ok(())
}

#[test]
fn inherited_git_selectors_cannot_certify_an_inert_ancestor() -> TestResult {
    let (sandbox, project, _sandbox_guard) = python_fixture("implicit-git-selectors")?;
    fs::create_dir(sandbox.join(".git"))?;
    let unrelated = sandbox.join("unrelated");
    fs::create_dir(&unrelated)?;
    run_git(&unrelated, &["init", "-b", "main"])?;
    let git_dir = unrelated.join(".git");
    let git_dir = git_dir.to_str().ok_or("fixture Git path is not UTF-8")?;
    let work_tree = sandbox.to_str().ok_or("fixture work tree is not UTF-8")?;
    let selectors = [
        ("GIT_DIR", git_dir),
        ("GIT_WORK_TREE", work_tree),
        ("GIT_COMMON_DIR", git_dir),
    ];
    let explicit = check(&project, true, &selectors)?;
    let implicit = check(&project, false, &selectors)?;
    assert_eq!(implicit["findings"], explicit["findings"]);
    fs::remove_dir_all(sandbox)?;
    Ok(())
}

#[test]
fn gitless_saved_diff_does_not_cross_a_nested_repository_boundary() -> TestResult {
    let (sandbox, project, _sandbox_guard) = python_fixture("implicit-gitless-boundary")?;
    run_git(&project, &["init", "-b", "main"])?;
    fs::write(sandbox.join("Cargo.toml"), "[workspace]\nmembers = []\n")?;
    let explicit = check(&project, true, &[("PATH", "")])?;
    let implicit = check(&project, false, &[("PATH", "")])?;
    assert_eq!(implicit["findings"], explicit["findings"]);
    fs::remove_dir_all(sandbox)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn git_launch_failure_stops_root_discovery_with_consumer_recovery() -> TestResult {
    let (sandbox, project, _sandbox_guard) = python_fixture("implicit-git-launch-failure")?;
    run_git(&project, &["init", "-b", "main"])?;
    fs::write(sandbox.join("Cargo.toml"), "[workspace]\nmembers = []\n")?;
    let programs = sandbox.join("programs");
    fs::create_dir(&programs)?;
    // A present, non-executable program is a launch failure, not missing Git.
    fs::write(programs.join("git"), "not executable\n")?;
    let path = programs.to_str().ok_or("fixture PATH is not UTF-8")?;
    let env = [("PATH", path), ("RIPR_CACHE_DIR", "")];
    for (args, recovery, expects_envelope) in [
        (
            vec!["check", "--diff", "change.diff", "--format", "json"],
            "pass --root PATH",
            true,
        ),
        (
            vec!["cache", "status", "--json"],
            "set RIPR_CACHE_DIR",
            false,
        ),
    ] {
        let output = run_command_with_env(env!("CARGO_BIN_EXE_ripr"), &project, &args, &env)?;
        assert_eq!(output.status.code(), Some(2));
        if expects_envelope {
            // #6834: `check --json` refusals emit the envelope on stdout;
            // `cache status --json` writes the shared refusal document
            // (#15260).
            let value = super::assert_check_json_refusal(
                &output,
                "repository_root_unusable",
                "analysis/repository-root",
            )?;
            let message = value["run_limitations"][0]["message"]
                .as_str()
                .ok_or("refusal must carry a message")?;
            assert!(
                message.contains("cannot verify implicit Git root"),
                "{message}"
            );
            assert!(message.contains(recovery), "{message}");
        } else {
            assert_cache_status_refusal_document(&output)?;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("cannot verify implicit Git root"),
            "{stderr}"
        );
        assert!(stderr.contains(recovery), "{stderr}");
        assert!(!stderr.contains("resolved workspace root to"), "{stderr}");
    }
    check(&project, true, &env)?;
    fs::remove_dir_all(sandbox)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn fixed_root_probe_timeout_names_only_effective_consumer_recovery() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let (sandbox, project, _sandbox_guard) = python_fixture("implicit-git-fixed-timeout")?;
    run_git(&project, &["init", "-b", "main"])?;
    let programs = sandbox.join("programs");
    fs::create_dir(&programs)?;
    let shim = programs.join("git");
    fs::write(&shim, "#!/bin/sh\nexec sleep 60\n")?;
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755))?;
    let path =
        std::env::join_paths([programs.as_path(), Path::new("/bin"), Path::new("/usr/bin")])?;
    let path = path.to_str().ok_or("fixture PATH is not UTF-8")?;
    let env = [
        ("PATH", path),
        ("RIPR_CACHE_DIR", ""),
        ("RIPR_GIT_TIMEOUT", "0"),
    ];
    for (args, recovery) in [
        (
            vec!["check", "--diff", "change.diff", "--git-timeout", "0"],
            "pass --root PATH",
        ),
        (vec!["cache", "status", "--json"], "set RIPR_CACHE_DIR"),
    ] {
        let output = run_command_with_env(env!("CARGO_BIN_EXE_ripr"), &project, &args, &env)?;
        assert_eq!(output.status.code(), Some(2));
        if args[0] == "cache" {
            assert_cache_status_refusal_document(&output)?;
        } else {
            assert!(output.stdout.is_empty());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("git_invocation_timeout:"), "{stderr}");
        assert!(
            stderr.contains("5000ms deadline (process terminated)"),
            "{stderr}"
        );
        assert!(stderr.contains(recovery), "{stderr}");
        for ineffective in ["--git-timeout", "RIPR_GIT_TIMEOUT", "gitTimeoutMs"] {
            assert!(!stderr.contains(ineffective), "{stderr}");
        }
    }
    fs::remove_dir_all(sandbox)?;
    Ok(())
}

/// #8767: from a workspace member directory `repair`, `pilot`, `status`,
/// `continue` and `agent stub` resolve the workspace root like `check`, and
/// `--root .` keeps the member directory.
#[cfg(unix)]
#[test]
fn task_first_commands_resolve_workspace_root_from_a_member_directory() -> TestResult {
    let outer = unique_external_workspace("member-root")?;
    let _cleanup = DeclaredHunkScratch(outer.clone());
    let member = outer.join("crates/app");
    fs::create_dir_all(member.join("src"))?;
    fs::write(
        outer.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/app\"]\n",
    )?;
    fs::write(
        member.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(member.join("src/lib.rs"), "pub fn price() -> u32 { 2 }\n")?;
    let root_text = outer.canonicalize()?.display().to_string();
    let disclosure = format!("ripr: resolved workspace root to {root_text} ");
    let run = |args: &[&str]| -> TestResult<std::process::Output> {
        Ok(run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &member,
            args,
            &[("RIPR_CACHE_DIR", "")],
        )?)
    };

    let pilot = run(&["pilot", "--quiet"])?;
    assert!(
        String::from_utf8_lossy(&pilot.stderr).contains(&disclosure),
        "pilot must disclose the resolved root; stderr:\n{}",
        String::from_utf8_lossy(&pilot.stderr)
    );
    assert!(
        outer.join("target/ripr").exists(),
        "pilot artifacts belong under the workspace target/ripr"
    );
    assert!(
        !member.join("target/ripr").exists(),
        "pilot must not create a member-local target/ripr"
    );

    // (args, expected exit): repair and continue report a named decision (3)
    // because the fixture has no repair-eligible gap or current attempt.
    for (args, expected) in [
        (&["repair"][..], 3),
        (&["status"][..], 0),
        (&["continue"][..], 3),
        (&["agent", "stub", "--at", "src/lib.rs:1"][..], 0),
    ] {
        let output = run(args)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{args:?} exit code; stderr:\n{stderr}"
        );
        assert!(
            stderr.contains(&disclosure),
            "{args:?} must disclose the resolved root; stderr:\n{stderr}"
        );
        let opted_out = run(&[args, &["--root", "."]].concat())?;
        assert!(
            !String::from_utf8_lossy(&opted_out.stderr).contains("resolved workspace root"),
            "{args:?} --root . must keep the member directory"
        );
    }

    fs::remove_dir_all(outer)?;
    Ok(())
}

/// #15473: from a subdirectory below the resolved workspace root, the human
/// `File:` paths are relative to the invocation directory, so a path copied
/// from the report opens from where the user ran `ripr`. JSON keeps the
/// workspace-relative `./` form, and a run at the root is unchanged.
#[cfg(unix)]
#[test]
fn human_file_paths_open_from_the_invocation_directory_below_the_root() -> TestResult {
    let outer = unique_external_workspace("human-path-cwd")?;
    let _cleanup = DeclaredHunkScratch(outer.clone());
    let core = outer.join("crates/core");
    let py = outer.join("py");
    fs::create_dir_all(core.join("src"))?;
    fs::create_dir_all(core.join("tests"))?;
    fs::create_dir_all(&py)?;
    fs::write(
        outer.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/core\"]\n",
    )?;
    fs::write(
        core.join("Cargo.toml"),
        "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        core.join("src/lib.rs"),
        "pub fn discount(amount: u32) -> u32 {\n    if amount >= 100 { 10 } else { 0 }\n}\n",
    )?;
    fs::write(
        core.join("tests/discount.rs"),
        "#[test]\nfn below() {\n    assert_eq!(core::discount(99), 0);\n}\n",
    )?;
    let diff = "--- a/crates/core/src/lib.rs\n+++ b/crates/core/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(amount: u32) -> u32 {\n-    if amount > 100 { 10 } else { 0 }\n+    if amount >= 100 { 10 } else { 0 }\n }\n";
    fs::write(py.join("change.diff"), diff)?;
    fs::write(outer.join("change.diff"), diff)?;
    run_git(&outer, &["init", "-b", "main"])?;

    let run = |dir: &Path, format: &str| -> TestResult<String> {
        let output = run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            dir,
            &[
                "check",
                "--diff",
                "change.diff",
                "--mode",
                "fast",
                "--format",
                format,
            ],
            &[("RIPR_CACHE_DIR", "")],
        )?;
        assert!(
            output.status.success(),
            "check failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    };
    let file_paths = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|line| line.trim_start().strip_prefix("File: "))
            .filter_map(|rest| rest.rsplit_once(':').map(|(path, _)| path.to_string()))
            .collect()
    };

    let from_subdir = file_paths(&run(&py, "human")?);
    assert!(!from_subdir.is_empty(), "fixture must print a File: row");
    for path in &from_subdir {
        assert_eq!(path, "../crates/core/src/lib.rs");
        assert!(
            py.join(path).is_file(),
            "`{path}` must resolve from the invocation directory"
        );
    }

    // Negative control: at the root the shared `./` spelling is unchanged.
    let at_root = file_paths(&run(&outer, "human")?);
    assert_eq!(
        at_root,
        vec!["./crates/core/src/lib.rs".to_string(); from_subdir.len()]
    );

    // JSON keeps its workspace-relative contract from the subdirectory.
    let json: Value = serde_json::from_str(&run(&py, "json")?)?;
    assert_eq!(
        json["findings"][0]["probe"]["file"],
        "./crates/core/src/lib.rs"
    );

    fs::remove_dir_all(outer)?;
    Ok(())
}

/// `cache status --json` refuses with the shared refusal document on stdout
/// (#15260): code `input_unusable`, the human cause as the message and the
/// command help as `next`.
#[cfg(unix)]
fn assert_cache_status_refusal_document(
    output: &std::process::Output,
) -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["status"], "refused", "{value:#}");
    assert_eq!(value["error"]["code"], "input_unusable", "{value:#}");
    assert_eq!(
        value["error"]["next"]["command"], "ripr cache status --help",
        "{value:#}"
    );
    Ok(())
}

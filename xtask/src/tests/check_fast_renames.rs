use std::path::Path;

/// Exercise both native selector callers against the same exact rename. The
/// fixture forces rename detection on so the regression does not depend on the
/// host's Git configuration. Only this newly created fixture is removed.
pub(crate) fn with_renamed_path(
    from: &str,
    to: &str,
    verify: impl Fn(&str, &[String]) -> Result<(), String>,
) -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-selector-rename-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        git(&root, &["init", "--initial-branch=main"])?;
        git(&root, &["config", "user.name", "Selector fixture"])?;
        git(&root, &["config", "user.email", "selector@example.invalid"])?;
        git(&root, &["config", "diff.renames", "true"])?;
        let old = root.join(from);
        std::fs::create_dir_all(old.parent().ok_or("source parent missing")?)
            .map_err(|error| error.to_string())?;
        std::fs::write(&old, "identical rename fixture payload\n")
            .map_err(|error| error.to_string())?;
        git(&root, &["add", "."])?;
        commit(&root, "base")?;
        git(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"])?;
        let new = root.join(to);
        std::fs::create_dir_all(new.parent().ok_or("destination parent missing")?)
            .map_err(|error| error.to_string())?;
        git(&root, &["mv", from, to])?;
        let status = git(
            &root,
            &["diff", "--cached", "--name-status", "--find-renames"],
        )?;
        assert_eq!(status.trim_end(), format!("R100\t{from}\t{to}"));
        verify("staged", &crate::git_changed_files(&root, "HEAD")?)?;
        commit(&root, "rename")?;
        verify("committed", &crate::changed_files_vs_base(&root)?)
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
    result.and(cleanup)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output =
        crate::run::capture_output_in_dir("git", &args, root, "selector rename fixture Git")?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!("fixture Git {args:?}: {}", output.stderr))
    }
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-m", message],
    )
    .map(|_| ())
}

#[test]
fn renames_retain_both_paths_and_original_applicability() -> Result<(), String> {
    // Literal category oracle: support, CLI, Rust, workflow, policy, fixture.
    // The CLI-to-analysis control retains Rust but must also retain CLI parity.
    for (from, to, expected) in [
        (
            "crates/ripr/tests/moved.rs",
            "docs/archive/rust.txt",
            [false, false, true, false, false, false],
        ),
        (
            "crates/ripr/src/cli/moved.rs",
            "crates/ripr/src/analysis/moved.rs",
            [false, true, true, false, false, false],
        ),
        (
            "policy/moved.toml",
            "docs/archive/policy.txt",
            [false, false, false, false, true, false],
        ),
        (
            ".ripr/moved.toml",
            "docs/archive/traceability.txt",
            [false, false, false, false, true, false],
        ),
        (
            ".github/workflows/moved.yml",
            "docs/archive/workflow.txt",
            [true, false, false, true, false, false],
        ),
        (
            "fixtures/moved.txt",
            "docs/archive/fixture.txt",
            [false, false, false, false, false, true],
        ),
        (
            "docs/specs/moved.md",
            "docs/archive/spec.txt",
            [true, false, false, false, false, false],
        ),
        ("docs/old.txt", "docs/archive/new.txt", [false; 6]),
    ] {
        with_renamed_path(from, to, |phase, paths| {
            let mut expected_paths = [from.to_string(), to.to_string()];
            expected_paths.sort();
            assert_eq!(paths, expected_paths, "{phase}: {from} -> {to}");
            let categories = crate::categorize_changed_files(paths);
            assert_eq!(
                [
                    categories.support_tiers,
                    categories.cli_help,
                    categories.rust_src,
                    categories.workflow,
                    categories.policy,
                    categories.fixture,
                ],
                expected,
                "{phase}: {from} -> {to}"
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn renamed_output_paths_still_select_goldens() -> Result<(), String> {
    for (from, to, selected) in [
        (
            "crates/ripr/src/output/moved.rs",
            "crates/ripr/src/analysis/moved.rs",
            true,
        ),
        (
            "fixtures/example/expected/moved.txt",
            "fixtures/example/input/moved.txt",
            true,
        ),
        ("docs/old.txt", "docs/new.txt", false),
    ] {
        with_renamed_path(from, to, |phase, paths| {
            assert_eq!(
                crate::golden_staleness_trigger(paths),
                selected.then_some(from),
                "{phase}: {from} -> {to}"
            );
            Ok(())
        })?;
    }
    Ok(())
}

// #13036: plain `ripr check` on a clean base branch must not dead-end: the
// zero-diff result names the comparison-base cause and prints the pasteable
// `ripr check --base HEAD~1` command; a stale `--base` that yields a
// tests-only diff is distinguished from "nothing changed".
use super::{assert_success, ignore_remove_dir_all, run_git, run_ripr, unique_temp_workspace};

/// The eval scenario (A/B #12912): the repo's last commit adds a function on
/// the only branch, so the default base resolves to HEAD itself.
#[test]
fn clean_base_check_names_the_cause_and_prints_the_previous_commit_command()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("clean-base-dead-end");
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"clean-base-dead-end\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    std::fs::write(root.join("src/lib.rs"), "pub fn existing() -> i32 { 1 }\n")?;
    run_git(&root, &["init", "-b", "master"])?;
    run_git(&root, &["config", "user.email", "ripr@example.invalid"])?;
    run_git(&root, &["config", "user.name", "ripr test"])?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "initial"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn existing() -> i32 { 1 }\n\npub fn added() -> i32 { 2 }\n",
    )?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "add a function"])?;
    let root_arg = root.display().to_string();

    let check = run_ripr(&["check", "--root", &root_arg]);
    assert_success(&check);
    let stdout = String::from_utf8_lossy(&check.stdout).into_owned();

    assert!(
        stdout.contains("Nothing to check: no files changed against the comparison base"),
        "zero-diff headline must name the comparison-base cause:\n{stdout}"
    );
    assert!(
        !stdout.contains("no changed source file in a language ripr analyzes"),
        "a zero-diff run must not name the language cause:\n{stdout}"
    );
    // The pasteable remediation, with the root bound because --root was used.
    assert!(
        stdout.contains("run: `ripr check --root ") && stdout.contains("--base HEAD~1`"),
        "the safe next action must print the pasteable previous-commit command:\n{stdout}"
    );
    assert!(
        !stdout.contains("which is the comparison base; edit a tracked source file"),
        "the dead-end edit advice must be replaced when HEAD has a parent:\n{stdout}"
    );

    // The printed command is genuinely runnable: it checks the last commit
    // against its parent and reports real behavior counts.
    let rerun = run_ripr(&["check", "--root", &root_arg, "--base", "HEAD~1"]);
    assert_success(&rerun);
    let rerun_out = String::from_utf8_lossy(&rerun.stdout).into_owned();
    assert!(
        !rerun_out.contains("Nothing to check"),
        "the advised command must analyze the last commit:\n{rerun_out}"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

/// The second eval failure: after committing their new tests, a stale
/// `--base HEAD~1` silently reported "0 changed behaviors checked" — a
/// tests-only diff must now say so and point at the feature commit.
#[test]
fn stale_base_with_tests_only_diff_is_distinguished_from_nothing_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("stale-base-tests-only");
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("tests"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"stale-base-tests-only\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    std::fs::write(root.join("src/lib.rs"), "pub fn price() -> i32 { 10 }\n")?;
    run_git(&root, &["init", "-b", "master"])?;
    run_git(&root, &["config", "user.email", "ripr@example.invalid"])?;
    run_git(&root, &["config", "user.name", "ripr test"])?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "initial"])?;
    // The feature commit: a production change.
    std::fs::write(root.join("src/lib.rs"), "pub fn price() -> i32 { 12 }\n")?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "feature change"])?;
    // The agent's own commit: tests only. A base at HEAD~1 sees just this.
    std::fs::write(
        root.join("tests/price.rs"),
        "#[test]\nfn price_is_positive() { assert!(crate::price() > 0); }\n",
    )?;
    run_git(&root, &["add", "."])?;
    run_git(&root, &["commit", "-m", "add tests"])?;
    let root_arg = root.display().to_string();

    let stale = run_ripr(&["check", "--root", &root_arg, "--base", "HEAD~1"]);
    assert_success(&stale);
    let stdout = String::from_utf8_lossy(&stale.stdout).into_owned();
    assert!(
        stdout.contains("Nothing to check: the diff changed test files only"),
        "a tests-only diff must be named as such:\n{stdout}"
    );
    assert!(
        stdout.contains("tests-only diffs carry no production behavior"),
        "the note must explain why tests-only is empty:\n{stdout}"
    );
    assert!(
        stdout.contains("--base HEAD~1"),
        "the note must print the pasteable previous-commit command:\n{stdout}"
    );
    assert!(
        !stdout.contains("No findings: ripr checked no changed behavior."),
        "a tests-only diff must not read as a checked-but-clean result:\n{stdout}"
    );

    // Moving the base back before the feature commit checks real behavior.
    let widened = run_ripr(&["check", "--root", &root_arg, "--base", "HEAD~2"]);
    assert_success(&widened);
    let widened_out = String::from_utf8_lossy(&widened.stdout).into_owned();
    assert!(
        !widened_out.contains("changed test files only"),
        "a base preceding the feature commit must not be reported tests-only:\n{widened_out}"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

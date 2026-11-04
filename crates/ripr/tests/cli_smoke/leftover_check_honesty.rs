// #11396 residuals: an empty scope never headlines "No findings", names no
// fixed language list, and a docs-only change is not explained twice.
use super::{init_git_fixture_repo, run_ripr, unique_temp_workspace};

#[test]
fn clean_tree_and_docs_only_changes_headline_what_was_not_examined()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("check-honesty");
    std::fs::create_dir_all(&root)?;
    init_git_fixture_repo(&root)?;
    let root_arg = root.display().to_string();

    let clean = run_ripr(&["check", "--root", &root_arg, "--quiet"]);
    let clean_out = String::from_utf8_lossy(&clean.stdout).into_owned();

    std::fs::write(root.join("marker.txt"), "fixture\nedited\n")?;
    let docs = run_ripr(&["check", "--root", &root_arg, "--quiet"]);
    let docs_out = String::from_utf8_lossy(&docs.stdout).into_owned();
    let docs_err = String::from_utf8_lossy(&docs.stderr).into_owned();
    let json = run_ripr(&["check", "--root", &root_arg, "--quiet", "--json"]);
    let json_err = String::from_utf8_lossy(&json.stderr).into_owned();
    let json_status = json.status.code();
    let docs_status = docs.status.code();
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        clean_out.contains("Nothing to check: the diff against the comparison base is empty"),
        "clean tree must name the empty diff, not the language: {clean_out}"
    );
    assert!(
        docs_out.contains("Nothing to check: no changed source file in a language ripr analyzes"),
        "docs only: {docs_out}"
    );
    for (label, out) in [("clean tree", &clean_out), ("docs only", &docs_out)] {
        assert!(!out.contains("No findings:"), "{label}: {out}");
        assert!(
            !out.contains("Rust, Python or TypeScript"),
            "{label} must not name a fixed language list: {out}"
        );
    }
    // The stderr note names what was skipped and keeps the non-claim for every
    // format; the headline adds the verdict for the default scope.
    assert_eq!(docs_status, Some(0), "{docs_err}");
    assert_eq!(json_status, Some(0), "{json_err}");
    for (label, err) in [("human", &docs_err), ("json", &json_err)] {
        assert!(
            err.contains("1 non-source file(s) (.txt)"),
            "{label} must name the count and extension: {err}"
        );
        assert!(
            err.contains(
                "Nothing was analyzed, so this says nothing about whether changed behavior is tested."
            ),
            "{label} must keep the non-claim: {err}"
        );
        assert!(!err.contains("Perl"), "{label}: {err}");
    }
    Ok(())
}

fn commit_all(root: &std::path::Path, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    super::run_git(root, &["add", "-A"])?;
    let commit = super::run_command(
        "git",
        Some(root),
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-m",
            message,
        ],
    )?;
    assert!(commit.status.success(), "commit failed: {commit:?}");
    Ok(())
}

// #13036: a zero-diff result on the comparison base names the empty diff and
// prints the paste-ready `--base HEAD~1`; a tests-only range says the base
// probably already includes the production change.
#[test]
fn empty_diff_and_tests_only_ranges_name_the_real_cause_and_command()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("check-empty-diff-hint");
    std::fs::create_dir_all(&root)?;
    init_git_fixture_repo(&root)?;
    super::run_git(&root, &["checkout", "-B", "main"])?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"sm\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn gate(x: u64) -> bool { x > 100 }\n",
    )?;
    commit_all(&root, "feature")?;
    let root_arg = root.display().to_string();

    let on_base = run_ripr(&["check", "--root", &root_arg, "--quiet"]);
    let on_base_out = String::from_utf8_lossy(&on_base.stdout).into_owned();

    std::fs::create_dir_all(root.join("tests"))?;
    std::fs::write(
        root.join("tests/t.rs"),
        "#[test]\nfn t() { assert!(true); }\n",
    )?;
    commit_all(&root, "tests")?;
    let tests_only = run_ripr(&["check", "--root", &root_arg, "--quiet", "--base", "HEAD~1"]);
    let tests_only_out = String::from_utf8_lossy(&tests_only.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        on_base_out.contains("the diff against the comparison base is empty"),
        "{on_base_out}"
    );
    assert!(
        on_base_out.contains("ripr check --base HEAD~1"),
        "{on_base_out}"
    );
    assert!(
        tests_only_out.contains("choose a base before that change"),
        "{tests_only_out}"
    );
    Ok(())
}

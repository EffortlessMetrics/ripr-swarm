// #11396 residuals: an empty scope never headlines "No findings", names no
// fixed language list, and a docs-only change is not explained twice.
// #13036: a zero-diff run names the comparison-base cause, not language
// availability.
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

    for (label, out) in [("clean tree", &clean_out), ("docs only", &docs_out)] {
        assert!(!out.contains("No findings:"), "{label}: {out}");
        assert!(
            !out.contains("Rust, Python or TypeScript"),
            "{label} must not name a fixed language list: {out}"
        );
    }
    // The fixture's single commit leaves HEAD on the resolved base: a plain
    // check is a zero-diff state and the headline names that real cause
    // (#13036), never the language-availability claim.
    assert!(
        clean_out.contains("Nothing to check: no files changed against the comparison base"),
        "clean tree: {clean_out}"
    );
    assert!(
        !clean_out.contains("no changed source file in a language ripr analyzes"),
        "clean tree must not name the language cause for a zero-diff run: {clean_out}"
    );
    // The uncommitted docs edit widens the run to a working-tree read of one
    // non-source file: the #11396 language-availability cause is the honest
    // headline there.
    assert!(
        docs_out.contains("Nothing to check: no changed source file in a language ripr analyzes"),
        "docs only: {docs_out}"
    );
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

//! History and filesystem controls shared by the installed selector and #14427.
//! Real Git objects reach the production API; no wheel build or hosted scheduling.
use super::{Fixture, LEDGER, TestResult};
use std::fs;
use std::os::unix::ffi::OsStringExt;

fn decision(fixture: &Fixture, base: &str, head: &str) -> TestResult<bool> {
    Ok(repo_policy::python_wheel::qualifies(
        &fixture.0,
        "pull_request",
        base,
        head,
    )?)
}

#[test]
fn package_feature_platform_and_license_deletions_retain_qualification() -> TestResult {
    let fixture = Fixture::new()?;
    let original = fixture.git(&["rev-parse", "HEAD"])?;
    // These are inputs of maturin, the release contract, or installed consumers.
    // Exercise removal, not just the additive unknown-path case already covered.
    for path in [
        "crates/ripr/Cargo.toml",
        "crates/ripr/build.rs",
        "crates/ripr/src/platform/windows.rs",
        "crates/ripr-contract/src/lib.rs",
        "packaging/python/pyproject.toml",
        "packaging/python/LICENSE-MIT",
        "policy/release-targets.toml",
        "crates/ripr/tests/boundary_gap_closure/ordinary_daily_loop.rs",
    ] {
        fixture.git(&["reset", "--hard", &original])?;
        let base = fixture.change(path, "retained package input\n")?;
        fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
        fixture.git(&["rm", "--", path])?;
        let head = fixture.commit()?;
        assert_eq!(
            fixture.git(&["diff", "--diff-filter=D", "--name-only", &base, &head])?,
            path
        );
        assert!(decision(&fixture, &base, &head)?, "deleted {path}");
    }
    Ok(())
}

#[test]
fn executable_mode_only_change_is_not_an_unrelated_ledger_edit() -> TestResult {
    let fixture = Fixture::new()?;
    let path = "packaging/python/install-control.sh";
    let base = fixture.change(path, "#!/bin/sh\nexit 0\n")?;
    let before = fixture.git(&["rev-parse", &format!("{base}:{path}")])?;
    fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    fixture.git(&["update-index", "--chmod=+x", "--", path])?;
    // Commit the index directly: Fixture::commit updates it from the worktree.
    fixture.git(&["commit", "-m", "mode only"])?;
    let head = fixture.git(&["rev-parse", "HEAD"])?;
    assert_eq!(
        fixture.git(&["rev-parse", &format!("{head}:{path}")])?,
        before
    );
    assert!(
        fixture
            .git(&["diff", "--summary", &base, &head])?
            .contains("mode change 100644 => 100755")
    );
    assert!(decision(&fixture, &base, &head)?);
    Ok(())
}

#[test]
fn newline_and_non_utf8_paths_cannot_hide_a_package_input() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    let head = fixture.change("packaging/python/line\nbreak.toml", "changed\n")?;
    assert!(decision(&fixture, &base, &head)?);
    fixture.assert_selection("pull_request", &base, &head, true)?;

    fixture.git(&["reset", "--hard", &base])?;
    fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    let path = std::ffi::OsString::from_vec(b"packaging/python/invalid-\xff.toml".to_vec());
    fs::create_dir_all(fixture.0.join("packaging/python"))?;
    fs::write(fixture.0.join(&path), "changed\n")?;
    // Git rejects non-UTF-8 argv before it can stage the path. Stage the
    // fixture root instead so the test reaches the history decoder (#14583).
    let added = fixture.command("git")?.args(["add", "-A"]).output()?;
    assert!(added.status.success(), "fixture add: {added:?}");
    let head = fixture.commit()?;
    let changed = fixture
        .command("git")?
        .args(["diff", "--name-only", "-z", &base, &head, "--"])
        .output()?;
    assert!(changed.status.success(), "fixture diff: {changed:?}");
    assert!(
        changed
            .stdout
            .windows(b"invalid-\xff".len())
            .any(|window| window == b"invalid-\xff"),
        "fixture did not retain the non-UTF-8 path: {changed:?}"
    );
    let Err(error) = repo_policy::python_wheel::qualifies(&fixture.0, "pull_request", &base, &head)
    else {
        return Err("undecodable history cannot prove a skip".into());
    };
    assert!(error.contains("not UTF-8"), "{error}");
    // The CLI's conservative error conversion is a separate observed boundary.
    fixture.assert_selection("pull_request", &base, &head, true)?;
    Ok(())
}

#[test]
fn criss_cross_history_refuses_even_when_each_possible_base_would_skip() -> TestResult {
    let fixture = Fixture::new()?;
    let root = fixture.git(&["rev-parse", "HEAD"])?;
    let ledger = fixture.ledger()?;
    let left = fixture.change(LEDGER, &format!("{ledger}\n# left\n"))?;
    fixture.git(&["checkout", "-b", "right", &root])?;
    let right = fixture.change(LEDGER, &format!("{ledger}\n# right\n"))?;
    let right_tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
    // Oppositely ordered merges share two best ancestors. No conflict resolution
    // or merge-base implementation is mocked; commit-tree constructs the graph.
    let first = fixture.git(&[
        "commit-tree",
        &right_tree,
        "-p",
        &left,
        "-p",
        &right,
        "-m",
        "first merge",
    ])?;
    let second = fixture.git(&[
        "commit-tree",
        &right_tree,
        "-p",
        &right,
        "-p",
        &left,
        "-m",
        "second merge",
    ])?;
    fixture.git(&["checkout", "--detach", &second])?;
    let head = fixture.change(LEDGER, &format!("{ledger}\n# candidate\n"))?;
    let ancestors = fixture.git(&["merge-base", "--all", &first, &head])?;
    let mut actual = ancestors.lines().collect::<Vec<_>>();
    actual.sort();
    let mut expected = vec![left.as_str(), right.as_str()];
    expected.sort();
    assert_eq!(actual, expected, "fixture must reach ambiguous ancestry");
    assert!(!decision(&fixture, &left, &head)?);
    assert!(!decision(&fixture, &right, &head)?);
    let Err(error) =
        repo_policy::python_wheel::qualifies(&fixture.0, "pull_request", &first, &head)
    else {
        return Err("no arbitrary merge base may authorize a skip".into());
    };
    assert!(error.contains("no unique merge base"), "{error}");
    fixture.assert_selection("pull_request", &first, &head, true)?;
    Ok(())
}

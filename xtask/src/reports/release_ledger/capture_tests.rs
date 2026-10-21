//! Capture against a small real Git repository: squash commits, a superseded
//! line, a deleted path, and a non-squash merge with a second-parent commit.

use super::capture::{CaptureArgs, GitFacts, capture, split_refs};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn git(root: &Path, values: &[&str]) -> Result<String, String> {
    let args = values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new("git"),
        &args,
        root,
        &[],
        &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
        Duration::from_secs(30),
        "2768 ledger capture fixture Git",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "fixture Git {values:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn commit(root: &Path, files: &[(&str, Option<&str>)], message: &str) -> Result<String, String> {
    for (path, contents) in files {
        match contents {
            Some(text) => fs::write(root.join(path), text).map_err(|error| error.to_string())?,
            None => fs::remove_file(root.join(path)).map_err(|error| error.to_string())?,
        }
    }
    git(root, &["add", "-A"])?;
    git(root, &["commit", "--quiet", "--allow-empty", "-m", message])?;
    git(root, &["rev-parse", "HEAD"])
}

fn fixture() -> Result<(PathBuf, GitFacts, Vec<String>), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-ledger-capture-{nonce}"));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    git(&root, &["init", "--quiet"])?;
    git(&root, &["config", "user.name", "test"])?;
    git(&root, &["config", "user.email", "test@example.invalid"])?;
    git(&root, &["checkout", "--quiet", "-b", "trunk"])?;
    let base = commit(
        &root,
        &[("a.txt", Some("1\n2\n3\n")), ("keep.txt", Some("k\n"))],
        "base",
    )?;
    let c1 = commit(
        &root,
        &[("a.txt", Some("one\n2\n3\n")), ("b.txt", Some("b\n"))],
        "feat: first (#11)",
    )?;
    let c2 = commit(
        &root,
        &[
            ("a.txt", Some("uno\n2\n3\n")),
            ("b.txt", None),
            ("c.txt", Some("c\n")),
        ],
        "fix: second, refs #5 (#12)",
    )?;
    let x = commit(&root, &[("temp.txt", Some("t\n"))], "chore: temp (#13)")?;
    let y = commit(&root, &[("temp.txt", None)], "chore: drop temp (#14)")?;
    git(&root, &["checkout", "--quiet", "-b", "side", &y])?;
    let s1 = commit(&root, &[("d.txt", Some("d\n"))], "feat: side commit")?;
    git(&root, &["checkout", "--quiet", "trunk"])?;
    let merge = commit(
        &root,
        &[("keep.txt", Some("k2\n"))],
        "feat: trunk edit (#15)",
    )?;
    git(
        &root,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "-m",
            "Merge pull request #16",
            "side",
        ],
    )?;
    let tip = git(&root, &["rev-parse", "HEAD"])?;
    let facts = capture(&CaptureArgs {
        repo: &root,
        repository: "example/ledger",
        base: &base,
        cut: &tip,
        main: &tip,
    })?;
    Ok((root, facts, vec![c1, c2, x, y, s1, merge, tip]))
}

fn effect<'a>(facts: &'a GitFacts, sha: &str) -> Result<&'a str, String> {
    facts
        .commits
        .iter()
        .find(|commit| commit.commit_sha == sha)
        .map(|commit| commit.final_tree_effect.as_str())
        .ok_or_else(|| format!("{sha} missing from facts"))
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

#[test]
fn capture_derives_exact_range_and_tree_effects() -> Result<(), String> {
    let (root, facts, shas) = fixture()?;
    let [c1, c2, x, y, s1, merge, tip] = shas.as_slice() else {
        return Err("fixture commit list changed".to_string());
    };
    let result = (|| {
        require(
            facts.range.first_parent == [c1, c2, x, y, merge, tip].map(String::clone),
            "first-parent order is wrong",
        )?;
        require(
            facts.range.reachable.last() == Some(s1),
            "the second-parent commit must follow the first-parent commits",
        )?;
        require(facts.commits.len() == 7, "one fact per reachable commit")?;
        require(
            effect(&facts, c1)? == "present_but_superseded",
            "c1's only line was replaced and b.txt was deleted",
        )?;
        require(
            effect(&facts, c2)? == "present_effective",
            "c2 last wrote a.txt",
        )?;
        require(
            effect(&facts, x)? == "history_reachable_no_final_blob_effect",
            "x only touched a path deleted later",
        )?;
        require(
            effect(&facts, y)? == "history_reachable_no_final_blob_effect",
            "y only deleted a path",
        )?;
        require(
            effect(&facts, s1)? == "present_effective",
            "the second-parent commit owns d.txt through blame",
        )?;
        let side = facts
            .commits
            .iter()
            .find(|commit| &commit.commit_sha == s1)
            .ok_or("s1 missing")?;
        require(
            side.first_parent_position.is_none() && side.effect_basis.blamed_lines == 1,
            "s1 is not first-parent and owns one blamed line",
        )?;
        require(
            facts
                .merged_prs
                .iter()
                .map(|pr| pr.number)
                .collect::<Vec<_>>()
                == [11, 12, 13, 14, 15],
            "squash PRs come from trailing subject refs only",
        )
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

#[test]
fn split_refs_takes_only_the_trailing_pull_request() -> Result<(), String> {
    require(
        split_refs("fix: x, refs #5 (#12)", true) == (vec![12], vec![5]),
        "trailing PR and a mention",
    )?;
    require(
        split_refs("docs: roadmap (#9278) (#9281)", true) == (vec![9281], vec![9278]),
        "only the last parenthesised ref is the squash PR",
    )?;
    require(
        split_refs("Merge pull request #16", true) == (vec![], vec![16]),
        "a merge subject has no trailing squash PR",
    )?;
    require(
        split_refs("feat: side (#7)", false) == (vec![], vec![7]),
        "a non-first-parent commit has no squash PR",
    )
}

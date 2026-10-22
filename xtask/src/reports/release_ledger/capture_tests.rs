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

fn new_repo(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-ledger-{label}-{nonce}"));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    git(&root, &["init", "--quiet"])?;
    git(&root, &["config", "user.name", "test"])?;
    git(&root, &["config", "user.email", "test@example.invalid"])?;
    git(&root, &["checkout", "--quiet", "-b", "trunk"])?;
    Ok(root)
}

fn fixture() -> Result<(PathBuf, GitFacts, Vec<String>), String> {
    let root = new_repo("capture")?;
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

#[test]
fn lines_written_under_a_renamed_path_are_attributed_by_blame() -> Result<(), String> {
    let root = new_repo("rename")?;
    let result = (|| {
        let base = commit(&root, &[("keep.txt", Some("k\n"))], "base")?;
        let writer = commit(
            &root,
            &[("old.txt", Some("one\ntwo\nthree\nfour\nfive\n"))],
            "feat: write (#21)",
        )?;
        git(&root, &["mv", "old.txt", "new.txt"])?;
        let mover = commit(&root, &[], "chore: move (#22)")?;
        let facts = capture(&CaptureArgs {
            repo: &root,
            repository: "example/ledger",
            base: &base,
            cut: &mover,
            main: &mover,
        })?;
        let found = facts
            .commits
            .iter()
            .find(|commit| commit.commit_sha == writer)
            .ok_or("writer missing")?;
        require(
            found.final_tree_effect == "present_effective"
                && found.effect_basis.alive_paths == 0
                && found.effect_basis.blamed_lines == 5,
            "the writer owns the five lines that now live under the new name",
        )
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

#[test]
fn newline_prefixed_file_names_keep_their_tree_effect() -> Result<(), String> {
    let root = new_repo("newline")?;
    let result = (|| {
        let base = commit(&root, &[("keep.txt", Some("k\n"))], "base")?;
        let writer = commit(&root, &[("\nnotes.txt", Some("n\n"))], "feat: notes (#31)")?;
        let facts = capture(&CaptureArgs {
            repo: &root,
            repository: "example/ledger",
            base: &base,
            cut: &writer,
            main: &writer,
        })?;
        let found = facts
            .commits
            .iter()
            .find(|commit| commit.commit_sha == writer)
            .ok_or("writer missing")?;
        require(
            found.paths == ["\nnotes.txt".to_string()]
                && found.final_tree_effect == "present_effective",
            "the newline-prefixed name survives byte for byte",
        )
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

#[test]
fn repeated_capture_flags_are_rejected_before_git_runs() -> Result<(), String> {
    let args: Vec<String> = ["capture", "--base", "a", "--base", "b", "--output", "x"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    let error = super::release_ledger(&args)
        .err()
        .ok_or("accepted a repeated flag")?;
    require(
        error.contains("more than once"),
        "the repeated flag is named",
    )
}

#[test]
fn inline_test_edits_are_detected_per_hunk_from_real_git() -> Result<(), String> {
    let root = new_repo("inline")?;
    let result = (|| {
        fs::create_dir_all(root.join("crates/ripr/src")).map_err(|error| error.to_string())?;
        let path = "crates/ripr/src/lib.rs";
        let base = commit(
            &root,
            &[(
                path,
                Some(
                    "pub fn run() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn runs() {\n        assert_eq!(super::run(), 1);\n    }\n}\n",
                ),
            )],
            "base",
        )?;
        let test_only = commit(
            &root,
            &[(
                path,
                Some(
                    "pub fn run() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn runs() {\n        assert_eq!(super::run(), 1);\n        assert_ne!(super::run(), 2);\n    }\n}\n",
                ),
            )],
            "test: tighten (#41)",
        )?;
        let product_only = commit(
            &root,
            &[(
                path,
                Some(
                    "pub fn run() -> u8 {\n    2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn runs() {\n        assert_eq!(super::run(), 1);\n        assert_ne!(super::run(), 2);\n    }\n}\n",
                ),
            )],
            "fix: change (#42)",
        )?;
        let mixed = commit(
            &root,
            &[(
                path,
                Some(
                    "pub fn run() -> u8 {\n    3\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn runs() {\n        assert_eq!(super::run(), 3);\n        assert_ne!(super::run(), 2);\n    }\n}\n",
                ),
            )],
            "fix: both (#43)",
        )?;
        let facts = capture(&CaptureArgs {
            repo: &root,
            repository: "example/ledger",
            base: &base,
            cut: &mixed,
            main: &mixed,
        })?;
        let inline = |sha: &str| -> Result<Vec<String>, String> {
            facts
                .commits
                .iter()
                .find(|commit| commit.commit_sha == sha)
                .map(|commit| commit.inline_test_paths.clone())
                .ok_or_else(|| format!("{sha} missing from facts"))
        };
        require(
            inline(&test_only)? == [path.to_string()],
            "a hunk inside the cfg(test) module is an inline-test edit",
        )?;
        require(
            inline(&product_only)?.is_empty(),
            "a hunk in product code is not",
        )?;
        require(
            inline(&mixed)?.is_empty(),
            "a hunk inside and a hunk outside the module keeps the file product",
        )
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

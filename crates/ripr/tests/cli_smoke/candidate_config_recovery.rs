use super::{
    assert_check_json_refusal, assert_success, common::fixture_git::fixture_git_output,
    ignore_remove_dir_all, init_check_refusal_repo, run_git, run_ripr,
};
use std::path::Path;
use std::process::Output;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const BROKEN_CONFIG: &str = "canary_candidate_config_9244 = [\n";
const RECOVERY: &str = "Fix the error above in the candidate's ripr.toml, or remove that file to use the built-in defaults; create a new candidate tree and rerun with its --candidate-tree value. Editing only the working-tree file does not change this candidate.";

fn stage_config(root: &Path, text: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::fs::write(root.join("ripr.toml"), text)?;
    run_git(root, &["add", "ripr.toml"])?;
    Ok(fixture_git_output(root, &["write-tree"])?.trim().to_owned())
}

fn check_candidate(root: &Path, tree: &str, format: &str) -> Output {
    run_ripr(&[
        "check",
        "--root",
        &root.display().to_string(),
        "--candidate-tree",
        tree,
        "--candidate-base",
        "HEAD",
        "--format",
        format,
    ])
}

#[test]
fn malformed_candidate_config_names_recovery_without_changing_json_refusal() -> TestResult {
    let root = init_check_refusal_repo("candidate-config-recovery", 1)?;
    let result = (|| {
        let tree = stage_config(&root, BROKEN_CONFIG)?;
        // A valid worktree cannot repair the immutable malformed subject.
        std::fs::write(root.join("ripr.toml"), "")?;
        for format in ["human", "json"] {
            let output = check_candidate(&root, &tree, format);
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            let stderr = String::from_utf8(output.stderr.clone())?;
            assert!(
                stderr.contains("candidate tree ripr.toml: invalid ripr.toml:"),
                "{stderr}"
            );
            assert!(stderr.contains("canary_candidate_config_9244"), "{stderr}");
            assert_eq!(stderr.lines().last(), Some(RECOVERY), "{stderr}");
            assert!(!stderr.contains("next ripr.toml it finds"), "{stderr}");
            if format == "json" {
                let value =
                    assert_check_json_refusal(&output, "config_invalid", "analysis/config-load")?;
                let message = value["run_limitations"][0]["message"]
                    .as_str()
                    .ok_or("missing refusal message")?;
                assert_eq!(
                    Some(message),
                    stderr
                        .lines()
                        .next()
                        .and_then(|line| line.strip_prefix("ripr: "))
                );
                let stdout = String::from_utf8(output.stdout)?;
                assert!(!stdout.contains("canary_candidate_config_9244"), "{stdout}");
                assert!(!stdout.contains(RECOVERY), "{stdout}");
            } else {
                assert!(output.stdout.is_empty(), "{output:?}");
            }
        }
        Ok(())
    })();
    ignore_remove_dir_all(&root);
    result
}

#[test]
fn repaired_and_config_free_candidates_ignore_a_broken_worktree_config() -> TestResult {
    let root = init_check_refusal_repo("candidate-config-recovery-control", 1)?;
    let result = (|| {
        std::fs::write(root.join("src/lib.rs"), "pub fn v0() -> u8 { 1 }\n")?;
        run_git(&root, &["add", "src/lib.rs"])?;
        let broken = stage_config(&root, BROKEN_CONFIG)?;
        let repaired = stage_config(&root, "[analysis]\nmode = \"draft\"\n")?;
        assert_ne!(
            broken, repaired,
            "repair must create a distinct immutable subject"
        );
        run_git(&root, &["rm", "--cached", "ripr.toml"])?;
        let defaults = fixture_git_output(&root, &["write-tree"])?
            .trim()
            .to_owned();
        assert_ne!(repaired, defaults);
        std::fs::write(root.join("ripr.toml"), BROKEN_CONFIG)?;
        for tree in [&repaired, &defaults] {
            let output = check_candidate(&root, tree, "json");
            assert_success(&output);
            let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            assert!(
                value["findings"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty()),
                "the repaired subject must analyze the changed function: {value}"
            );
            assert!(!String::from_utf8(output.stderr)?.contains(RECOVERY));
        }
        // A new tree does not mutate or repair the old object's result.
        let output = check_candidate(&root, &broken, "json");
        assert_check_json_refusal(&output, "config_invalid", "analysis/config-load")?;
        Ok(())
    })();
    ignore_remove_dir_all(&root);
    result
}

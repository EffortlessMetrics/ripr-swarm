//! Execute the real matcher-selection shell with bounded Cargo/Git stand-ins.
//! This checks trusted-source selection and exit propagation; matcher semantics
//! remain covered by the repo-policy frontdoor's real compiled matcher tests.
use super::{WORKFLOW, step_id};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn workflow_step(id: &str) -> Result<&'static str, Box<dyn std::error::Error>> {
    WORKFLOW
        .split("\n      - ")
        .find(|step| step_id(step) == Some(id))
        .ok_or_else(|| "user-text step missing".into())
}

fn workflow_script(id: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(workflow_step(id)?
        .split_once("        run: |\n")
        .ok_or("user-text script missing")?
        .1
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[test]
fn matcher_uses_only_trusted_base_capability_and_propagates_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let script = workflow_script("user_text_matcher")?;
    for (case, lightweight, tree_capable, native_exit, expected_command) in [
        ("lightweight", true, true, 0, "policy"),
        ("legacy", false, true, 0, "xtask"),
        ("lightweight-failed", true, true, 23, "policy"),
        ("legacy-failed", false, true, 23, "xtask"),
        ("pre-tree", false, false, 0, ""),
    ] {
        let root =
            std::env::temp_dir().join(format!("ripr-base-matcher-{}-{case}", std::process::id()));
        fs::create_dir(&root)?;
        let scratch = Scratch(root);
        let base = scratch.0.join("user-text-base");
        let candidate = scratch.0.join("candidate");
        let bin = scratch.0.join("bin");
        for tree in [&base, &candidate] {
            fs::create_dir_all(tree.join("tools/repo-policy/src"))?;
            fs::create_dir_all(tree.join("xtask/src/policy"))?;
        }
        fs::create_dir(&bin)?;
        fs::write(
            base.join("xtask/src/policy/user_text.rs"),
            if tree_capable { "\"--tree\"" } else { "legacy" },
        )?;
        if lightweight {
            fs::write(
                base.join("tools/repo-policy/src/main.rs"),
                "Some(\"check-user-text\") => check_foreign_tree(),\n",
            )?;
        }
        // A candidate's capability must never switch a legacy base's judge.
        fs::write(
            candidate.join("tools/repo-policy/src/main.rs"),
            "Some(\"check-user-text\") => accept_everything(),\n",
        )?;
        for (name, body) in [
            (
                "git",
                "#!/bin/bash\n[[ $1 == -C && $2 == \"$BASE\" && $3 == rev-parse && $4 == HEAD ]] || exit 97\nprintf '%040d\\n' 1\n",
            ),
            (
                "cargo",
                "#!/bin/bash\n[[ $PWD == \"$BASE\" ]] || exit 98\nprintf '%s\\n' \"$*\" >> \"$TRACE\"\nexit \"$NATIVE_EXIT\"\n",
            ),
        ] {
            let path = bin.join(name);
            fs::write(&path, body)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
        let trace = scratch.0.join("trace");
        let output = Command::new("timeout")
            .args([
                "--kill-after=1s",
                "5s",
                "bash",
                "-eo",
                "pipefail",
                "-c",
                &script,
            ])
            .current_dir(&candidate)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("BASE", &base)
            .env("TRACE", &trace)
            .env("NATIVE_EXIT", native_exit.to_string())
            .env("RUNNER_TEMP", &scratch.0)
            .env("GITHUB_WORKSPACE", &candidate)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(native_exit),
            "{case}: {output:?}"
        );
        if expected_command.is_empty() {
            assert!(!trace.exists());
            assert!(String::from_utf8_lossy(&output.stdout).contains("matcher pin only"));
        } else {
            assert_eq!(
                fs::read_to_string(trace)?,
                format!(
                    "{expected_command} check-user-text --tree {}\n",
                    candidate.display()
                ),
                "{case}: exactly one trusted judge must run"
            );
        }
    }
    Ok(())
}

fn bounded_command(
    root: &Path,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    Ok(Command::new("timeout")
        .args(["--kill-after=1s", "10s", program])
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", std::env::var("PATH")?)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .envs(envs.iter().copied())
        .output()?)
}

struct JudgeHistory {
    scratch: Scratch,
    repository: PathBuf,
    base: String,
    earlier: String,
    group: String,
}

impl JudgeHistory {
    fn git(&self, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let output = bounded_command(&self.repository, "git", args, &[])?;
        assert!(output.status.success(), "git {args:?}: {output:?}");
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    fn commit(&self, path: &str, body: &str) -> Result<String, Box<dyn std::error::Error>> {
        fs::write(self.repository.join(path), body)?;
        self.git(&["add", path])?;
        self.git(&["-c", "commit.gpgsign=false", "commit", "-qm", path])?;
        self.git(&["rev-parse", "HEAD"])
    }

    fn new(case: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("ripr-judge-base-{}-{case}", std::process::id()));
        fs::create_dir(&root)?;
        let mut history = Self {
            repository: root.join("candidate"),
            scratch: Scratch(root),
            base: String::new(),
            earlier: String::new(),
            group: String::new(),
        };
        fs::create_dir(&history.repository)?;
        history.git(&["init", "--quiet", "--initial-branch=main"])?;
        history.base = history.commit("judge.txt", "protected base judge\n")?;
        history.earlier = history.commit("judge.txt", "unlanded constituent judge\n")?;
        history.group = history.commit("second.txt", "second queued constituent\n")?;
        assert_eq!(history.git(&["rev-parse", "HEAD^1"])?, history.earlier);
        assert_ne!(history.base, history.earlier);
        Ok(history)
    }

    fn prepare(
        &self,
        case: &str,
        envs: &[(&str, &str)],
    ) -> Result<(std::process::Output, PathBuf), Box<dyn std::error::Error>> {
        let runner = self.scratch.0.join(case);
        fs::create_dir(&runner)?;
        let runner_text = runner.to_string_lossy();
        let mut bindings = vec![
            ("EVENT_NAME", "merge_group"),
            ("EVENT_SHA", self.group.as_str()),
            ("GROUP_BASE_SHA", self.base.as_str()),
            ("GROUP_HEAD_SHA", self.group.as_str()),
            ("PR_BASE_SHA", ""),
            ("PR_HEAD_SHA", ""),
            ("RUNNER_TEMP", runner_text.as_ref()),
        ];
        bindings.extend_from_slice(envs);
        let script = workflow_script("user_text_base")?;
        let output = bounded_command(
            &self.repository,
            "bash",
            &["-eo", "pipefail", "-c", &script],
            &bindings,
        )?;
        Ok((output, runner.join("user-text-base")))
    }
}

#[test]
fn judge_preparation_uses_group_base_before_every_unlanded_constituent()
-> Result<(), Box<dyn std::error::Error>> {
    let step = workflow_step("user_text_base")?;
    for binding in [
        "          EVENT_NAME: ${{ github.event_name }}",
        "          EVENT_SHA: ${{ github.sha }}",
        "          PR_BASE_SHA: ${{ github.event.pull_request.base.sha }}",
        "          PR_HEAD_SHA: ${{ github.event.pull_request.head.sha }}",
        "          GROUP_BASE_SHA: ${{ github.event.merge_group.base_sha }}",
        "          GROUP_HEAD_SHA: ${{ github.event.merge_group.head_sha }}",
    ] {
        assert_eq!(
            step.lines().filter(|line| *line == binding).count(),
            1,
            "{binding}"
        );
    }
    let history = JudgeHistory::new("group")?;
    let (output, judge) = history.prepare("accepted", &[])?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(judge.join("judge.txt"))?,
        "protected base judge\n"
    );
    let selected = bounded_command(&judge, "git", &["rev-parse", "HEAD"], &[])?;
    assert!(selected.status.success(), "{selected:?}");
    assert_eq!(String::from_utf8(selected.stdout)?.trim(), history.base);
    Ok(())
}

#[test]
fn judge_preparation_retains_verified_pr_merge_parent_with_stale_event_base()
-> Result<(), Box<dyn std::error::Error>> {
    let history = JudgeHistory::new("pr")?;
    history.git(&["checkout", "-qb", "pull-request", &history.base])?;
    let pr_head = history.commit("pr.txt", "candidate\n")?;
    history.git(&["checkout", "-qb", "advanced-base", &history.base])?;
    let actual_base = history.commit("base-moved.txt", "accepted base movement\n")?;
    history.git(&[
        "-c",
        "commit.gpgsign=false",
        "merge",
        "--no-ff",
        "pull-request",
        "-qm",
        "synthetic merge",
    ])?;
    let merge = history.git(&["rev-parse", "HEAD"])?;
    let bindings = [
        ("EVENT_NAME", "pull_request"),
        ("EVENT_SHA", merge.as_str()),
        ("PR_BASE_SHA", history.base.as_str()),
        ("PR_HEAD_SHA", pr_head.as_str()),
    ];
    let (output, judge) = history.prepare("accepted", &bindings)?;
    assert!(output.status.success(), "{output:?}");
    let selected = bounded_command(&judge, "git", &["rev-parse", "HEAD"], &[])?;
    assert!(selected.status.success(), "{selected:?}");
    assert_eq!(String::from_utf8(selected.stdout)?.trim(), actual_base);
    for (case, key, value) in [
        ("missing-pr-base", "PR_BASE_SHA", ""),
        ("wrong-pr-head", "PR_HEAD_SHA", history.base.as_str()),
        ("unrelated-pr-base", "PR_BASE_SHA", history.group.as_str()),
    ] {
        let mut wrong = bindings.to_vec();
        wrong.push((key, value));
        let (output, judge) = history.prepare(case, &wrong)?;
        assert_eq!(output.status.code(), Some(1), "{case}: {output:?}");
        assert!(!judge.exists(), "{case}: created an unbound judge");
    }
    history.git(&["checkout", "--detach", &pr_head])?;
    let mut wrong = bindings.to_vec();
    wrong.push(("EVENT_SHA", pr_head.as_str()));
    let (output, judge) = history.prepare("not-a-merge", &wrong)?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("two-parent merge"));
    assert!(!judge.exists());
    Ok(())
}

#[test]
fn judge_preparation_refuses_unbound_or_unavailable_group_identities()
-> Result<(), Box<dyn std::error::Error>> {
    let history = JudgeHistory::new("refusals")?;
    history.git(&["checkout", "--orphan", "unrelated"])?;
    history.git(&["rm", "-qrf", "."])?;
    let unrelated = history.commit("other.txt", "unrelated history\n")?;
    history.git(&["checkout", "--detach", &history.group])?;
    let missing = "f".repeat(40);
    assert_ne!(missing, history.base);
    for (case, key, value) in [
        ("missing-event", "EVENT_SHA", ""),
        ("wrong-event", "EVENT_SHA", history.earlier.as_str()),
        ("missing-base", "GROUP_BASE_SHA", ""),
        ("malformed-base", "GROUP_BASE_SHA", "main"),
        ("unavailable-base", "GROUP_BASE_SHA", missing.as_str()),
        ("unrelated-base", "GROUP_BASE_SHA", unrelated.as_str()),
        ("self-base", "GROUP_BASE_SHA", history.group.as_str()),
        ("missing-head", "GROUP_HEAD_SHA", ""),
        ("wrong-head", "GROUP_HEAD_SHA", history.earlier.as_str()),
        ("wrong-event-kind", "EVENT_NAME", "push"),
    ] {
        let (output, judge) = history.prepare(case, &[(key, value)])?;
        assert_eq!(output.status.code(), Some(1), "{case}: {output:?}");
        assert!(!judge.exists(), "{case}: created an unbound judge");
    }
    Ok(())
}

use std::error::Error;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn generated_workflow_batches_compact_review_comments() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-review-workflow-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["init", "--root"])
        .arg(&root)
        .args(["--ci", "github"])
        .output()?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let review_endpoint = "gh api --method POST \"repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/reviews\"";
    let legacy_create_endpoint = "gh api --method POST \"repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/comments\"";
    let update_endpoint =
        "gh api --method PATCH \"repos/${{ github.repository }}/pulls/comments/$comment_id\"";
    assert_eq!(workflow.matches(review_endpoint).count(), 1);
    assert!(!workflow.contains(legacy_create_endpoint));
    assert!(workflow.contains(update_endpoint));
    assert!(workflow.contains("event: \"COMMENT\""));
    assert!(workflow.contains("comments: ["));
    assert!(workflow.contains("<details><summary>Full RIPR repair card</summary>"));
    assert!(workflow.contains("presentation=compact-v1"));
    // #3906: a card that carries the repair start leads the compact comment
    // with it; only cards without one fall back to the Verify line.
    assert!(
        workflow.contains(r#"captured("\nStart the repair:\n`(?<value>[^`]+)`"; "")) as $start"#)
    );
    assert!(workflow.contains(
        r#"(if $start then "Start the repair: `\($start)`" else "Verify: `\($verify)`" end) as $next"#
    ));
    assert!(workflow.contains("__ripr_legacy_presentation__"));
    assert!(workflow.contains("__ripr_compact_presentation_unreadable__"));
    assert!(workflow.contains("additional recommendation"));
    assert!(workflow.contains("target/ripr/review/comments.json"));
    assert!(workflow.contains("target/ripr/review/comments.md"));
    assert!(workflow.contains("Created one RIPR review with $create_count inline comment(s)."));

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #3906 (F60-1, F60-2, F60-3, F60-14): replay the generated workflow's `run:`
/// steps, in order, on a PR-shaped fixture repository, then hold the job
/// summary to what a new adopter can act on.
///
/// - Every executed step exits 0. The agent-loop step used to exit 2 on every
///   run (verify had no movement between two snapshots of one HEAD) and left
///   a 0-byte `agent-verify.json` in the upload.
/// - No JSON artifact under `target/ripr` is empty or unparseable.
/// - The `First-run status` block leads with the repair start that
///   start-here carries, instead of `Safe next action command: none`.
/// - Every `ripr ...` command the summary prints either runs as printed
///   (exit 0) from a fresh copy of the post-CI workspace, or is labelled as a
///   step that runs after the test edit or the repair.
#[cfg(unix)]
#[test]
fn generated_workflow_replay_prints_only_runnable_next_steps() -> Result<(), Box<dyn Error>> {
    for tool in ["bash", "git", "jq"] {
        if !replay::tool_available(tool) {
            eprintln!(
                "SKIPPED generated_workflow_replay_prints_only_runnable_next_steps: `{tool}` is not on PATH; the generated workflow needs it"
            );
            return Ok(());
        }
    }
    let base = replay::unique_temp_dir("replay")?;
    let root = base.join("repo");
    replay::write_pr_fixture(&root)?;

    let init = replay::ripr(&root, &["init", "--root", ".", "--ci", "github"])?;
    assert!(
        init.status.success(),
        "ripr init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    replay::git(&root, &["add", "-A"])?;
    replay::git(&root, &["commit", "-q", "-m", "add ripr advisory workflow"])?;

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let steps = replay::parse_steps(&workflow);
    assert!(
        steps.len() > 20,
        "workflow step parser found {} steps; the template layout changed",
        steps.len()
    );
    let runs = replay::run_workflow(&root, &base, &steps)?;

    // Precondition: the agent-loop step ran for a real top seam.
    let agent_loop = runs
        .iter()
        .find(|run| run.name == "Generate RIPR agent loop artifacts")
        .ok_or("the agent-loop step did not run; the fixture produced no top seam")?;
    assert_eq!(
        agent_loop.exit_code,
        Some(0),
        "agent-loop step failed:\n{}",
        agent_loop.output
    );
    let failed = runs
        .iter()
        .filter(|run| run.exit_code != Some(0))
        .map(|run| format!("{} (exit {:?}):\n{}", run.name, run.exit_code, run.output))
        .collect::<Vec<_>>();
    assert!(
        failed.is_empty(),
        "workflow steps failed:\n{}",
        failed.join("\n")
    );

    // No artifact the upload step would ship is an empty or invalid JSON.
    let json_files = replay::json_files(&root.join("target/ripr"))?;
    assert!(
        json_files.len() > 10,
        "only {} JSON artifacts were written",
        json_files.len()
    );
    for path in &json_files {
        let text = fs::read_to_string(path)?;
        assert!(
            !text.trim().is_empty(),
            "empty JSON artifact: {}",
            path.display()
        );
        if let Err(err) = serde_json::from_str::<serde_json::Value>(&text) {
            return Err(format!("invalid JSON artifact {}: {err}", path.display()).into());
        }
    }
    assert!(
        root.join("target/ripr/workflow/agent-packet.json")
            .is_file(),
        "the agent-loop step must still write the packet the repair starts from"
    );
    assert!(
        !root.join("target/ripr/workflow/agent-verify.json").exists(),
        "CI has no test edit between snapshots, so it must not write a verify artifact"
    );

    // Precondition: start-here carries the repair start the summary leads with.
    let start_here: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        root.join("target/ripr/reports/start-here.json"),
    )?)?;
    assert_eq!(start_here["selected"]["state"], "top_gap", "{start_here}");
    let repair_command = start_here["selected"]["repair_command"]
        .as_str()
        .ok_or("start-here.json carries no selected.repair_command")?;
    assert!(
        repair_command.starts_with("ripr agent repair --root . --seam-id ")
            && repair_command.ends_with(" --phase before"),
        "{repair_command}"
    );

    let summary = fs::read_to_string(base.join("step-summary.md"))?;
    let first_block = summary
        .split("#### First-run status\n")
        .nth(1)
        .ok_or("summary has no First-run status block")?;
    assert!(
        first_block.starts_with(&format!("- Start repair: `{repair_command}`\n")),
        "the First-run status block must lead with the repair start:\n{}",
        first_block.lines().take(4).collect::<Vec<_>>().join("\n")
    );
    assert!(
        !summary.contains("Safe next action command: `none`"),
        "summary still says there is no safe next action"
    );

    let commands = replay::printed_commands(&summary);
    let runnable = commands
        .iter()
        .filter(|command| !command.after_edit)
        .map(|command| command.text.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let deferred = commands
        .iter()
        .filter(|command| command.after_edit)
        .map(|command| command.text.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        runnable.contains(repair_command),
        "the repair start must be printed as runnable now: {runnable:?}"
    );
    assert!(
        !deferred
            .iter()
            .any(|command| command.contains("--phase before")),
        "a before-phase command must not be labelled as a post-edit step: {deferred:?}"
    );
    for label in [
        "ripr agent verify ",
        "ripr agent receipt ",
        "ripr assistant-loop proof ",
    ] {
        assert!(
            deferred.iter().any(|command| command.starts_with(label)),
            "expected the summary to print `{label}...` as a post-edit step: {deferred:?}"
        );
    }

    // Steps that compare against an after snapshot have nothing to compare
    // before the test edit, even when the command itself exits 0.
    let premature = runnable
        .iter()
        .filter(|command| {
            command.contains("after.repo-exposure.json")
                || command.starts_with("ripr agent verify ")
                || command.starts_with("ripr agent receipt ")
                || command.starts_with("ripr outcome ")
        })
        .collect::<Vec<_>>();
    assert!(
        premature.is_empty(),
        "post-edit steps are printed as if they could run now: {premature:?}"
    );

    let mut failures = Vec::new();
    for (index, command) in runnable.iter().enumerate() {
        let copy = base.join(format!("fresh-{index}"));
        replay::copy_tree(&root, &copy)?;
        let output = replay::bash(&copy, command, &[])?;
        if !output.status.success() {
            failures.push(format!(
                "`{command}` exited {:?}:\n{}{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "summary commands printed as runnable failed as printed:\n{}",
        failures.join("\n")
    );

    // The assistant-proof regeneration is labelled as post-repair, but it
    // must still be a complete command: a bare `--out` form exits 2 because
    // the proof refuses to run without explicit inputs.
    for (index, command) in deferred
        .iter()
        .filter(|command| command.starts_with("ripr assistant-loop proof "))
        .enumerate()
    {
        let copy = base.join(format!("proof-{index}"));
        replay::copy_tree(&root, &copy)?;
        let output = replay::bash(&copy, command, &[])?;
        assert!(
            output.status.success(),
            "`{command}` is not a complete command: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // The repair start also runs from a pristine clone with no CI artifacts.
    let clone = base.join("clone");
    replay::git(
        &base,
        &[
            "clone",
            "-q",
            root.to_str().ok_or("non-utf8 path")?,
            clone.to_str().ok_or("non-utf8 path")?,
        ],
    )?;
    let started = replay::bash(&clone, repair_command, &[])?;
    assert!(
        started.status.success(),
        "`{repair_command}` failed from a fresh clone: {}",
        String::from_utf8_lossy(&started.stderr)
    );

    fs::remove_dir_all(base)?;
    Ok(())
}

#[cfg(unix)]
mod replay {
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    type TestResult<T> = Result<T, Box<dyn Error>>;

    const LIB_BASE: &str = "pub const DISCOUNT_THRESHOLD: u64 = 10_000;

pub fn discounted_total(amount: u64) -> u64 {
    if amount > DISCOUNT_THRESHOLD {
        amount - amount / 10
    } else {
        amount
    }
}
";

    const TESTS: &str = "use pricing::discounted_total;

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(5_000), 5_000);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(20_000), 18_000);
}
";

    /// Phrases that mark a printed command as a step for after the test edit
    /// or the repair, rather than one to run on the checkout as it is.
    const AFTER_EDIT_MARKERS: &[&str] = &[
        "after the test edit",
        "after the focused test edit",
        "after adding one focused test",
        "after verify",
        "without a repair attempt",
        "after the repair's after phase",
    ];

    #[derive(Debug, Default)]
    pub(super) struct Step {
        pub(super) name: String,
        condition: Option<String>,
        run: Option<String>,
        env: Vec<(String, String)>,
    }

    pub(super) struct StepRun {
        pub(super) name: String,
        pub(super) exit_code: Option<i32>,
        pub(super) output: String,
    }

    pub(super) struct PrintedCommand {
        pub(super) text: String,
        pub(super) after_edit: bool,
    }

    pub(super) fn tool_available(tool: &str) -> bool {
        Command::new(tool)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    pub(super) fn unique_temp_dir(label: &str) -> TestResult<PathBuf> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ripr-generated-workflow-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// A one-crate repo whose PR moves `>` to `>=` on a named threshold that
    /// the tests never exercise at the boundary: a repair-ready gap.
    pub(super) fn write_pr_fixture(root: &Path) -> TestResult<()> {
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir_all(root.join("tests"))?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )?;
        fs::write(root.join("src/lib.rs"), LIB_BASE)?;
        fs::write(root.join("tests/pricing.rs"), TESTS)?;
        git(root, &["init", "-q", "-b", "main"])?;
        git(root, &["add", "-A"])?;
        git(root, &["commit", "-q", "-m", "initial pricing crate"])?;
        git(root, &["update-ref", "refs/remotes/origin/main", "HEAD"])?;
        git(root, &["checkout", "-q", "-b", "feature"])?;
        fs::write(
            root.join("src/lib.rs"),
            LIB_BASE.replace(
                "amount > DISCOUNT_THRESHOLD",
                "amount >= DISCOUNT_THRESHOLD",
            ),
        )?;
        git(
            root,
            &["commit", "-q", "-a", "-m", "discount at the threshold"],
        )?;
        Ok(())
    }

    pub(super) fn git(dir: &Path, args: &[&str]) -> TestResult<()> {
        let output = Command::new("git")
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "ripr test")
            .env("GIT_AUTHOR_EMAIL", "ripr-test@example.invalid")
            .env("GIT_COMMITTER_NAME", "ripr test")
            .env("GIT_COMMITTER_EMAIL", "ripr-test@example.invalid")
            .stdin(Stdio::null())
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into())
        }
    }

    pub(super) fn ripr(dir: &Path, args: &[&str]) -> TestResult<Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .output()?)
    }

    fn path_with_ripr() -> TestResult<String> {
        let bin = Path::new(env!("CARGO_BIN_EXE_ripr"))
            .parent()
            .ok_or("ripr binary has no parent directory")?;
        let inherited = std::env::var("PATH").unwrap_or_default();
        Ok(format!("{}:{inherited}", bin.display()))
    }

    /// Run a script the way a GitHub-hosted Linux step does without a
    /// `shell:` key (`bash -e {0}`), with the freshly built `ripr` first on
    /// PATH.
    pub(super) fn bash(dir: &Path, script: &str, env: &[(String, String)]) -> TestResult<Output> {
        let mut command = Command::new("bash");
        command
            .args(["--noprofile", "--norc", "-e", "-c", script])
            .current_dir(dir)
            .env("PATH", path_with_ripr()?)
            .stdin(Stdio::null());
        for (key, value) in env {
            command.env(key, value);
        }
        Ok(command.output()?)
    }

    pub(super) fn copy_tree(from: &Path, to: &Path) -> TestResult<()> {
        let status = Command::new("cp")
            .arg("-a")
            .arg(from)
            .arg(to)
            .stdin(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("cp -a {} {} failed", from.display(), to.display()).into())
        }
    }

    /// Parse the generated workflow's job steps. The template is regular:
    /// steps open at six spaces (`- `), keys sit at eight, and `run: |`
    /// bodies and step `env:` entries at ten.
    pub(super) fn parse_steps(workflow: &str) -> Vec<Step> {
        let lines = workflow.lines().collect::<Vec<_>>();
        let Some(start) = lines.iter().position(|line| *line == "    steps:") else {
            return Vec::new();
        };
        let mut steps: Vec<Step> = Vec::new();
        let mut index = start + 1;
        while index < lines.len() {
            let line = lines[index];
            index += 1;
            let entry = if let Some(rest) = line.strip_prefix("      - ") {
                steps.push(Step::default());
                rest
            } else if let Some(rest) = line.strip_prefix("        ") {
                if rest.starts_with(' ') || rest.starts_with('#') {
                    continue;
                }
                rest
            } else {
                continue;
            };
            let Some(step) = steps.last_mut() else {
                continue;
            };
            let Some((key, value)) = entry.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key {
                "name" => step.name = value.to_string(),
                "uses" if step.name.is_empty() => step.name = value.to_string(),
                "if" => step.condition = Some(value.to_string()),
                "run" if value == "|" => {
                    let mut body = Vec::new();
                    while index < lines.len() {
                        let next = lines[index];
                        if next.trim().is_empty() {
                            body.push("");
                        } else if let Some(stripped) = next.strip_prefix("          ") {
                            body.push(stripped);
                        } else {
                            break;
                        }
                        index += 1;
                    }
                    while body.last() == Some(&"") {
                        body.pop();
                    }
                    step.run = Some(body.join("\n"));
                }
                "run" => step.run = Some(value.to_string()),
                "env" => {
                    while index < lines.len() {
                        let Some(pair) = lines[index].strip_prefix("          ") else {
                            break;
                        };
                        if let Some((name, value)) = pair.split_once(':') {
                            step.env
                                .push((name.trim().to_string(), value.trim().to_string()));
                        }
                        index += 1;
                    }
                }
                _ => {}
            }
        }
        steps
    }

    fn github_expression(expression: &str, head_sha: &str) -> Option<String> {
        let value = match expression {
            "github.base_ref" => "main",
            "github.event_name" => "pull_request",
            "github.repository" => "ripr-test/pricing",
            "github.event.number" | "github.event.pull_request.number" => "1",
            "github.event.pull_request.head.repo.full_name" => "ripr-test/pricing",
            "github.event.pull_request.head.sha" => head_sha,
            "github.token" => "replay-token-unused",
            "vars.RIPR_GATE_MODE == '' || vars.RIPR_GATE_MODE == 'visible-only'" => "true",
            _ => return None,
        };
        Some(value.to_string())
    }

    fn substitute(text: &str, head_sha: &str) -> TestResult<String> {
        let mut out = String::new();
        let mut rest = text;
        while let Some(open) = rest.find("${{") {
            out.push_str(&rest[..open]);
            let after = &rest[open + 3..];
            let close = after.find("}}").ok_or("unterminated workflow expression")?;
            let expression = after[..close].trim();
            let value = github_expression(expression, head_sha)
                .ok_or_else(|| format!("replay does not model `${{{{ {expression} }}}}`"))?;
            out.push_str(&value);
            rest = &after[close + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }

    fn operand(text: &str, root: &Path, env: &BTreeMap<String, String>) -> TestResult<String> {
        let text = text.trim();
        if let Some(path) = text
            .strip_prefix("hashFiles('")
            .and_then(|rest| rest.strip_suffix("')"))
        {
            return Ok(if root.join(path).exists() {
                "hash".to_string()
            } else {
                String::new()
            });
        }
        if let Some(literal) = text
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            return Ok(literal.to_string());
        }
        if let Some(name) = text.strip_prefix("env.") {
            return Ok(env.get(name).cloned().unwrap_or_default());
        }
        if text == "github.event_name" {
            return Ok("pull_request".to_string());
        }
        Err(format!("replay does not model the condition operand `{text}`").into())
    }

    fn condition_holds(
        condition: &str,
        root: &Path,
        env: &BTreeMap<String, String>,
    ) -> TestResult<bool> {
        for atom in condition.split("&&").map(str::trim) {
            let holds = if atom == "always()" {
                true
            } else if let Some((left, right)) = atom.split_once("!=") {
                operand(left, root, env)? != operand(right, root, env)?
            } else if let Some((left, right)) = atom.split_once("==") {
                operand(left, root, env)? == operand(right, root, env)?
            } else {
                return Err(format!("replay does not model the condition `{atom}`").into());
            };
            if !holds {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Replay every `run:` step in order. `uses:` steps and the crates.io
    /// install are skipped: the checkout is the fixture and `ripr` is the
    /// binary under test. `$GITHUB_ENV` writes carry into later steps.
    pub(super) fn run_workflow(
        root: &Path,
        base: &Path,
        steps: &[Step],
    ) -> TestResult<Vec<StepRun>> {
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()?;
        let head_sha = String::from_utf8(head.stdout)?.trim().to_string();
        let github_env = base.join("github-env");
        let summary = base.join("step-summary.md");
        let event = base.join("event.json");
        fs::write(&github_env, "")?;
        fs::write(&summary, "")?;
        fs::write(base.join("github-output"), "")?;
        fs::write(
            &event,
            format!(
                "{{\"pull_request\":{{\"number\":1,\"labels\":[],\"head\":{{\"sha\":\"{head_sha}\"}}}}}}"
            ),
        )?;
        let mut env = BTreeMap::from([
            ("RIPR_UPLOAD_SARIF".to_string(), "true".to_string()),
            ("RIPR_GATE_MODE".to_string(), String::new()),
            ("RIPR_GATE_BASELINE".to_string(), String::new()),
            ("RIPR_COMMENT_MODE".to_string(), "off".to_string()),
            ("GITHUB_ENV".to_string(), github_env.display().to_string()),
            (
                "GITHUB_STEP_SUMMARY".to_string(),
                summary.display().to_string(),
            ),
            (
                "GITHUB_OUTPUT".to_string(),
                base.join("github-output").display().to_string(),
            ),
            ("GITHUB_EVENT_PATH".to_string(), event.display().to_string()),
            ("GITHUB_EVENT_NAME".to_string(), "pull_request".to_string()),
            ("GITHUB_BASE_REF".to_string(), "main".to_string()),
            ("GITHUB_HEAD_REF".to_string(), "feature".to_string()),
            (
                "GITHUB_REPOSITORY".to_string(),
                "ripr-test/pricing".to_string(),
            ),
            ("GITHUB_SHA".to_string(), head_sha.clone()),
            ("GITHUB_WORKSPACE".to_string(), root.display().to_string()),
            ("RUNNER_TEMP".to_string(), base.display().to_string()),
        ]);
        let mut runs = Vec::new();
        for step in steps {
            let Some(script) = &step.run else {
                continue;
            };
            if script.starts_with("cargo install ripr") {
                continue;
            }
            if let Some(condition) = &step.condition
                && !condition_holds(condition, root, &env)?
            {
                continue;
            }
            let script = substitute(script, &head_sha)?;
            let mut step_env = env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>();
            for (name, value) in &step.env {
                step_env.push((name.clone(), substitute(value, &head_sha)?));
            }
            let output = bash(root, &script, &step_env)?;
            runs.push(StepRun {
                name: step.name.clone(),
                exit_code: output.status.code(),
                output: format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
            for line in fs::read_to_string(&github_env)?.lines() {
                if let Some((name, value)) = line.split_once('=') {
                    env.insert(name.to_string(), value.to_string());
                }
            }
        }
        Ok(runs)
    }

    pub(super) fn json_files(dir: &Path) -> TestResult<Vec<PathBuf>> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in fs::read_dir(&next)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "json") {
                    found.push(path);
                }
            }
        }
        Ok(found)
    }

    /// Code spans on one Markdown line, honoring `\`` escapes, with the byte
    /// offset where each span opens.
    fn code_spans(line: &str) -> Vec<(usize, String)> {
        let mut spans = Vec::new();
        let mut current: Option<(usize, String)> = None;
        let mut chars = line.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            if ch == '\\' && chars.peek().is_some_and(|(_, next)| *next == '`') {
                if let Some((_, span)) = current.as_mut() {
                    span.push('`');
                }
                chars.next();
            } else if ch == '`' {
                match current.take() {
                    Some(span) => spans.push(span),
                    None => current = Some((offset, String::new())),
                }
            } else if let Some((_, span)) = current.as_mut() {
                span.push(ch);
            }
        }
        spans
    }

    /// The nearest preceding line that labels a command, skipping blank
    /// lines, fences, and the shared shell disclosures.
    fn label_before(lines: &[&str], index: usize) -> String {
        lines[..index]
            .iter()
            .rev()
            .map(|line| line.trim())
            .find(|line| {
                !line.is_empty()
                    && !line.starts_with("```")
                    && !line.starts_with("Each command includes")
                    && !line.starts_with("The first form is written")
            })
            .unwrap_or_default()
            .to_string()
    }

    fn after_edit(label: &str) -> bool {
        let label = label.to_ascii_lowercase();
        AFTER_EDIT_MARKERS
            .iter()
            .any(|marker| label.contains(marker))
    }

    /// Every `ripr` command line the summary prints: bash fence lines and
    /// code spans that start with `ripr ` and carry a flag. Bare mentions
    /// such as `ripr gate evaluate` in prose are not commands to run.
    pub(super) fn printed_commands(summary: &str) -> Vec<PrintedCommand> {
        let lines = summary.lines().collect::<Vec<_>>();
        let mut commands = Vec::new();
        let mut fence: Option<(bool, String)> = None;
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if let Some(info) = trimmed.strip_prefix("```") {
                fence = match fence {
                    Some(_) => None,
                    None => Some((info == "bash", label_before(&lines, index))),
                };
                continue;
            }
            if let Some((is_bash, label)) = &fence {
                if *is_bash && trimmed.starts_with("ripr ") {
                    commands.push(PrintedCommand {
                        text: trimmed.to_string(),
                        after_edit: after_edit(label),
                    });
                }
                continue;
            }
            for (offset, span) in code_spans(line) {
                if !span.starts_with("ripr ") || !span.contains(" --") {
                    continue;
                }
                let before = line[..offset].trim();
                let label = if before.is_empty() || before == "-" {
                    label_before(&lines, index)
                } else {
                    before.to_string()
                };
                commands.push(PrintedCommand {
                    text: span,
                    after_edit: after_edit(&label),
                });
            }
        }
        commands
    }
}

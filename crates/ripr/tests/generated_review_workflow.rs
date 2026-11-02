use std::error::Error;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};

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
    // #5409: ripr builds the requests; the step replays them with `gh api`
    // against the repository only, and never builds a body in shell.
    let publish = workflow_step_block(&workflow, "Publish RIPR inline comments")
        .ok_or("missing publish step")?;
    assert!(publish.contains("PR_NUMBER: ${{ github.event.pull_request.number }}"));
    assert!(publish.contains("PR_HEAD_SHA: ${{ github.event.pull_request.head.sha }}"));
    assert!(publish.contains("REPO: ${{ github.repository }}"));
    assert!(publish.contains(
        "ripr pr-comments requests --root . --pull-request \"$PR_NUMBER\" --head-sha \"$PR_HEAD_SHA\""
    ));
    assert!(publish.contains(
        "gh api --method \"$method\" \"repos/$REPO/$endpoint\" --input \"$request\" </dev/null >/dev/null"
    ));
    assert_no_github_expression_in_run_blocks(&workflow);
    assert!(!workflow.contains("jq "));

    let update_body = "### ripr gap: stale\n\nRepair:\nAdd one.\n\nVerify:\n`ripr agent verify`";
    let plan = serde_json::json!({
        "summary": {"safe_to_publish": true, "publishable": 3, "summary_only": 0, "suppressed": 0},
        "operations": [
            {"operation": "create", "safe_to_publish": true, "dedupe_key": "ripr:a",
             "placement": {"path": "src/lib.rs", "line": 1}, "body": "### ripr gap: a\n\nRepair:\nAdd a.\n"},
            {"operation": "update", "safe_to_publish": true, "dedupe_key": "ripr:b",
             "existing_comment_id": 77, "body": update_body},
            {"operation": "create", "safe_to_publish": true, "dedupe_key": "ripr:c",
             "placement": {"path": "src/lib.rs", "line": 9, "side": "RIGHT"}, "body": "### ripr gap: c\n"}
        ]
    });
    let (requests, stdout) = run_pr_comments_requests(&root, &plan)?;
    let calls: Vec<(&str, &str)> = requests
        .iter()
        .map(|request| (request.method.as_str(), request.endpoint.as_str()))
        .collect();
    // Updates go first, then exactly one review creates every new card; the
    // legacy one-comment-per-call endpoint is never used.
    assert_eq!(
        calls,
        vec![("PATCH", "pulls/comments/77"), ("POST", "pulls/42/reviews")]
    );
    let review = &requests[1].payload;
    assert_eq!(review["event"], "COMMENT");
    assert_eq!(review["commit_id"], "0123abcd");
    assert_eq!(review["comments"].as_array().map(Vec::len), Some(2));
    let created = review["comments"][0]["body"].as_str().unwrap_or_default();
    assert!(
        created.contains("<details><summary>Full RIPR repair card</summary>"),
        "{created}"
    );
    assert!(
        created.ends_with("<!-- ripr:dedupe=ripr:a presentation=compact-v1 -->"),
        "{created}"
    );
    assert!(
        requests[0].payload["body"]
            .as_str()
            .unwrap_or_default()
            .starts_with("**ripr: stale** — Add one.\n\nVerify: `ripr agent verify`")
    );
    assert_eq!(
        requests[1].message,
        "Created one RIPR review with 2 inline comment(s)."
    );
    assert_eq!(requests[0].message, "Updated RIPR inline comment: ripr:b");
    assert!(stdout.is_empty(), "{stdout}");

    // An unsafe plan writes no request at all and says why.
    let unsafe_plan = serde_json::json!({
        "summary": {"safe_to_publish": false},
        "operations": plan["operations"].clone(),
        "blocked": [{"blocked_reason": "missing_write_permission", "message": "read-only token"}]
    });
    let (requests, stdout) = run_pr_comments_requests(&root, &unsafe_plan)?;
    assert!(requests.is_empty(), "{requests:?}");
    assert!(
        stdout.contains("- missing_write_permission: read-only token"),
        "{stdout}"
    );

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
            // A hosted runner always has these tools, so a missing one there
            // is a broken runner, not a reason to skip the strongest oracle
            // for the generated workflow.
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                return Err(format!(
                    "`{tool}` is not on PATH under GitHub Actions; the generated-workflow replay cannot be skipped in CI"
                )
                .into());
            }
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
        steps.iter().any(|step| step.name == "Run RIPR"),
        "workflow step parser found no `Run RIPR` step; the template layout changed"
    );
    let runs = replay::run_workflow(&root, &base, &steps)?;
    // #11145: no upload failed in the replay (it skips every `uses:` step), so
    // the failed-upload explanation is skipped; its text, run on its own, is
    // a valid workflow command that names the fix and exits 0.
    assert!(
        runs.iter()
            .all(|run| run.name != "Explain failed SARIF upload"),
        "the explanation must not run when no upload failed"
    );
    let explain = steps
        .iter()
        .find(|step| step.name == "Explain failed SARIF upload")
        .and_then(|step| step.run.as_deref())
        .ok_or("generated workflow has no `Explain failed SARIF upload` run step")?;
    let explained = replay::bash(&root, explain, &[])?;
    let explained_out = String::from_utf8_lossy(&explained.stdout);
    assert!(explained.status.success(), "{explained_out}");
    assert!(
        explained_out.starts_with("::warning title=RIPR SARIF upload failed::")
            && explained_out.contains("RIPR_UPLOAD_SARIF to false"),
        "{explained_out}"
    );
    // Precondition: the packet command printed its steps as log groups.
    assert!(
        runs.iter().filter(|run| run.name != "Run RIPR").count() > 20,
        "`ripr reports ci-packet` printed too few step groups"
    );

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
    // Each full report is collapsed under its at-a-glance lines: every
    // `Full report` opener has its own closer, and no report's top-level
    // heading (the workflow's own headings are `##` and deeper) is visible
    // outside a collapsed block.
    let openers = summary.matches("<details><summary>Full report: ").count();
    assert!(openers > 0, "summary collapses no full report:\n{summary}");
    // The governance fold is a `<details>` block too, so compare every opener.
    assert_eq!(
        summary.matches("<details").count(),
        summary.matches("</details>").count(),
        "every collapsed block must close"
    );
    let mut collapsed = false;
    for line in summary.lines() {
        if line.starts_with("<details><summary>Full report: ") {
            collapsed = true;
        } else if line == "</details>" {
            collapsed = false;
        } else if !collapsed {
            assert!(
                !line.starts_with("# "),
                "a full report's heading is visible outside its collapsed block: {line}"
            );
        }
    }

    // The annotation GitHub places on the changed line carries the same
    // repair start, and nothing that points into this runner's checkout
    // (F60-7): the reader is on another machine.
    let annotations = runs
        .iter()
        .find(|run| run.name == "Emit RIPR PR guidance annotations")
        .ok_or("the annotation step did not run")?;
    let warnings = annotations
        .output
        .lines()
        .filter(|line| line.starts_with("::warning "))
        .collect::<Vec<_>>();
    assert!(
        !warnings.is_empty(),
        "no annotation was emitted:\n{}",
        annotations.output
    );
    assert!(
        warnings
            .iter()
            .any(|line| line.ends_with(&format!(" Start the repair: {repair_command}"))),
        "no annotation names the repair start:\n{}",
        warnings.join("\n")
    );
    let checkout = root.display().to_string();
    for line in &warnings {
        assert!(
            !line.contains(&checkout) && !line.contains("agent brief"),
            "annotation points into the runner checkout or at the brief: {line}"
        );
    }

    // Every at-a-glance block whose artifact carries the same repair start
    // leads with it and its after phase (#3906, F60-14). Precondition: each
    // artifact really carries the command, so the block is held to it.
    for (artifact, pointer, heading) in [
        (
            "target/ripr/reports/first-useful-action.json",
            "/commands/repair",
            "#### Recommended next test at a glance\n",
        ),
        (
            "target/ripr/reports/pr-review-front-panel.json",
            "/top_issue/repair_command",
            "#### PR review at a glance\n",
        ),
        (
            "target/ripr/reports/pr-evidence-ledger.json",
            "/top_repair_route/repair_command",
            "#### PR movement at a glance\n",
        ),
    ] {
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join(artifact))?)?;
        assert_eq!(
            value.pointer(pointer).and_then(serde_json::Value::as_str),
            Some(repair_command),
            "{artifact} must carry the repair start at {pointer}"
        );
        let block = summary
            .split(heading)
            .nth(1)
            .ok_or_else(|| format!("summary has no {heading:?} block"))?;
        assert!(
            block.starts_with(&format!(
                "- Repair start: `{repair_command}`\n- After the test edit: run the `--attempt ... --phase after` command"
            )),
            "{heading:?} must lead with the repair start:\n{}",
            block.lines().take(4).collect::<Vec<_>>().join("\n")
        );
    }

    // Before any test edit the agent review packet names the missing
    // receipt as expected and leads with the same repair start and its
    // after phase (#3906, N5), instead of the review summary's post-edit
    // snapshot loop with checkout-absolute redirect paths.
    let packet = summary
        .split("### Agent review packet\n")
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .ok_or("summary has no Agent review packet block")?;
    assert!(
        packet.starts_with(&format!(
            "- Receipt: No agent receipt yet. None is expected before a repair: the `--attempt ... --phase after` command writes it after the focused test edit.\n- Start repair: `{repair_command}`\n- After the test edit: run the `--attempt ... --phase after` command"
        )),
        "the review packet must lead with the repair start:\n{packet}"
    );
    let root_text = root.to_str().ok_or("non-utf8 path")?;
    for stale in [
        "Movement: missing_artifact",
        "Run the next command listed by agent status",
        root_text,
    ] {
        assert!(
            !packet.contains(stale),
            "the review packet must not print `{stale}` before a repair:\n{packet}"
        );
    }

    // The PR review summary and Recommended next test blocks print commands
    // a reader copies on another machine, so they never name this runner's
    // checkout. `generated_summary_prints_repository_relative_commands`
    // holds the rewrite to artifacts that do carry the checkout path.
    for heading in ["### PR review summary\n", "### Recommended next test\n"] {
        let block = summary
            .split(heading)
            .nth(1)
            .and_then(|rest| rest.split("\n### ").next())
            .ok_or_else(|| format!("summary has no {heading:?} block"))?;
        assert!(
            !block.contains(root_text),
            "{heading:?} prints the runner checkout path:\n{block}"
        );
    }

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

/// #3937/#3999/#4000: the seam-selection command `agent status` generates
/// in repository A, pasted into Bash from an unrelated directory B, analyzes
/// and writes A, and the next status step reads what it wrote. B holds a
/// decoy checkout under the same relative name, so a command that re-resolved
/// the relative `--root` at the paste site would analyze the decoy while its
/// anchored `--out` still wrote under A — the split this binding removes.
#[cfg(unix)]
#[test]
fn generated_status_command_runs_from_a_foreign_working_directory() -> Result<(), Box<dyn Error>> {
    for tool in ["bash", "git"] {
        if !replay::tool_available(tool) {
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                return Err(format!("`{tool}` is not on PATH under GitHub Actions").into());
            }
            eprintln!(
                "SKIPPED generated_status_command_runs_from_a_foreign_working_directory: `{tool}` is not on PATH"
            );
            return Ok(());
        }
    }
    let base = replay::unique_temp_dir("rooted-foreign-cwd")?;
    let parent = base.join("sélected parent");
    let repo = parent.join("repo root");
    let foreign = base.join("foreign cwd");
    let decoy = foreign.join("repo root");
    replay::write_pr_fixture(&repo)?;
    replay::write_pr_fixture(&decoy)?;

    // Render: status runs from A's parent with a relative --root, the way a
    // user in a monorepo parent directory would select the repository.
    let status = |dir: &std::path::Path| -> Result<serde_json::Value, Box<dyn Error>> {
        let output = replay::ripr(dir, &["agent", "status", "--root", "repo root", "--json"])?;
        assert!(
            output.status.success(),
            "agent status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let before = status(&parent)?;
    assert_eq!(before["next_command"]["step"], "select_seam", "{before}");
    let command = before["next_command"]["command"]
        .as_str()
        .ok_or("status next command is not a string")?
        .to_string();

    // Paste: run the exact generated text from B.
    let run = replay::bash(&foreign, &command, &[])?;
    assert!(
        run.status.success(),
        "generated command failed from a foreign directory: {command}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    // Placement: the artifacts land under A and nothing lands in B.
    let pilot_dir = repo.join("target/ripr/pilot");
    let snapshot: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(pilot_dir.join("repo-exposure.json"))?)?;
    let analyzed_root = snapshot
        .pointer("/artifact/repository/root")
        .and_then(serde_json::Value::as_str)
        .ok_or("repo-exposure snapshot names no repository root")?;
    // Subject: the snapshot analyzed A, not B's decoy of the same name.
    assert_eq!(
        std::path::Path::new(analyzed_root).canonicalize()?,
        repo.canonicalize()?,
        "{command} analyzed the wrong repository"
    );
    assert!(pilot_dir.join("pilot-summary.json").is_file());
    assert!(
        !foreign.join("target").exists() && !decoy.join("target").exists(),
        "{command} wrote under the paste directory"
    );

    // Consumption: the next status step reads the artifact the pasted
    // command wrote and routes to the repair start bound to A.
    let after = status(&parent)?;
    assert_eq!(
        after["next_command"]["step"], "repair_attempt_before",
        "{after}"
    );
    let repair = after["next_command"]["command"]
        .as_str()
        .ok_or("repair command is not a string")?;
    assert!(
        repair.contains(&format!("--root '{}'", repo.canonicalize()?.display())),
        "repair start must bind A: {repair}"
    );

    fs::remove_dir_all(base)?;
    Ok(())
}

/// #4000: doctor's current/stale packet refresh binds its selected root and
/// survives literal Bash replay after the user changes working directory.
#[cfg(unix)]
#[test]
fn doctor_packet_refresh_runs_from_a_foreign_working_directory() -> Result<(), Box<dyn Error>> {
    for tool in ["bash", "git"] {
        if !replay::tool_available(tool) {
            return Err(format!("`{tool}` is required for doctor refresh replay").into());
        }
    }
    let base = replay::unique_temp_dir("doctor-refresh-foreign-cwd")?;
    let parent = base.join("selected parent");
    let name = "repo é's $notes";
    let repo = parent.join(name);
    let foreign = base.join("foreign cwd");
    let decoy = foreign.join(name);
    replay::write_pr_fixture(&repo)?;
    replay::write_pr_fixture(&decoy)?;
    // No origin: the real resolver must select the local master branch;
    // neither origin/main nor a shell <ref> placeholder can stand in for it.
    replay::git(&repo, &["branch", "-m", "trunk", "master"])?;
    replay::git(&repo, &["update-ref", "-d", "refs/remotes/origin/trunk"])?;
    let produced = replay::ripr(&repo, &["first-pr", "--base", "master"])?;
    assert!(
        produced.status.success(),
        "{}",
        String::from_utf8_lossy(&produced.stderr)
    );
    let reports = repo.join("target/ripr/reports");
    let json_path = reports.join("start-here.json");
    let markdown_path = reports.join("start-here.md");

    for stale in [false, true] {
        for (cwd, root) in [(&parent, name), (&repo, ".")] {
            let mut before: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&json_path)?)?;
            before["ripr_version"] = if stale {
                "0.0.0"
            } else {
                env!("CARGO_PKG_VERSION")
            }
            .into();
            fs::write(&json_path, serde_json::to_string_pretty(&before)?)?;
            let doctor = replay::ripr(cwd, &["doctor", "--root", root])?;
            assert!(
                doctor.status.success(),
                "{}",
                String::from_utf8_lossy(&doctor.stderr)
            );
            let stdout = String::from_utf8(doctor.stdout)?;
            assert_eq!(stdout.contains("stale_evidence"), stale, "{stdout}");
            let command = stdout
                .lines()
                .find(|line| line.starts_with("- Safe next action:"))
                .and_then(|line| line.split('`').nth(1))
                .ok_or("doctor emitted no packet refresh command")?;

            // Force the pasted command to recreate the selected packet. An
            // old artifact under A cannot mask a command that refreshed B.
            fs::remove_file(&markdown_path)?;
            let refreshed = replay::bash(&foreign, command, &[])?;
            assert!(
                refreshed.status.success(),
                "doctor refresh failed (stale={stale}, root={root}): {command}\nstdout={}\nstderr={}",
                String::from_utf8_lossy(&refreshed.stdout),
                String::from_utf8_lossy(&refreshed.stderr)
            );
            assert!(
                markdown_path.is_file(),
                "refresh must recreate the selected packet"
            );
            let after: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json_path)?)?;
            let refreshed_root = after["root"].as_str().ok_or("packet omitted root")?;
            assert_eq!(
                std::path::Path::new(refreshed_root).canonicalize()?,
                repo.canonicalize()?
            );
            assert_eq!(after["inputs"]["base"], "master", "{after}");
            assert_eq!(after["inputs"]["head"], "HEAD", "{after}");
            assert_eq!(after["ripr_version"], env!("CARGO_PKG_VERSION"));
            assert!(
                !foreign.join("target").exists() && !decoy.join("target").exists(),
                "refresh must not write under the paste directory or its decoy"
            );
            let powershell_root = repo.to_string_lossy().replace('\'', "''");
            assert!(stdout.contains(&format!(
                "- Refresh command (PowerShell): ripr first-pr --root '{powershell_root}' --head HEAD"
            )), "apostrophe path needs the shared PowerShell spelling: {stdout}");
        }
    }
    // When the selected repository no longer has a default base, the same
    // generic command must retain first-pr's refusal and preserve the packet.
    replay::git(&repo, &["branch", "-m", "master", "topic-base"])?;
    let doctor = replay::ripr(&parent, &["doctor", "--root", name])?;
    assert!(doctor.status.success());
    let stdout = String::from_utf8(doctor.stdout)?;
    let command = stdout
        .lines()
        .find(|line| line.starts_with("- Safe next action:"))
        .and_then(|line| line.split('`').nth(1))
        .ok_or("doctor emitted no packet refresh command")?;
    let before_json = fs::read(&json_path)?;
    let before_markdown = fs::read(&markdown_path)?;
    let refused = replay::bash(&foreign, command, &[])?;
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("could not resolve a default base"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(fs::read(&json_path)?, before_json);
    assert_eq!(fs::read(&markdown_path)?, before_markdown);
    assert!(!foreign.join("target").exists() && !decoy.join("target").exists());
    fs::remove_dir_all(base)?;
    Ok(())
}

/// #4000/#4991: the emitted refresh must name the same physical directory
/// whose packet doctor read, even when the input traverses a symlink and `..`.
/// Inspect the route without executing a possibly misdirected refresh.
#[cfg(unix)]
#[test]
fn doctor_packet_refresh_and_first_check_preserve_physical_root_identity()
-> Result<(), Box<dyn Error>> {
    let base = replay::unique_temp_dir("doctor-refresh-physical-root")?;
    let parent = base.join("selected parent");
    let physical = base.join("physical parent");
    let repo = physical.join("repo");
    let decoy = parent.join("repo");
    replay::write_pr_fixture(&repo)?;
    replay::write_pr_fixture(&decoy)?;
    fs::create_dir_all(physical.join("child"))?;
    std::os::unix::fs::symlink(physical.join("child"), parent.join("link"))?;
    let relative = std::path::Path::new("link/../repo");
    let absolute = parent.join(relative);
    let selected = repo.canonicalize()?;
    assert_eq!(absolute.canonicalize()?, selected);
    assert_ne!(decoy.canonicalize()?, selected);
    let reports = "target/ripr/reports";
    for root in [&repo, &decoy] {
        let produced = replay::ripr(root, &["first-pr", "--base", "origin/trunk"])?;
        assert!(
            produced.status.success(),
            "{}",
            String::from_utf8_lossy(&produced.stderr)
        );
    }
    for stale in [false, true] {
        // Opposite versions prove which packet the doctor actually read.
        for (root, old) in [(&repo, stale), (&decoy, !stale)] {
            let path = root.join(reports).join("start-here.json");
            let mut packet: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
            packet["ripr_version"] = if old {
                "0.0.0"
            } else {
                env!("CARGO_PKG_VERSION")
            }
            .into();
            fs::write(path, serde_json::to_string_pretty(&packet)?)?;
        }
        for spelling in [relative, absolute.as_path()] {
            let paths = [&repo, &decoy]
                .into_iter()
                .flat_map(|root| {
                    ["start-here.json", "start-here.md"].map(|name| root.join(reports).join(name))
                })
                .collect::<Vec<_>>();
            let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
            let doctor = replay::ripr(&parent, &["doctor", "--root", &spelling.to_string_lossy()])?;
            assert!(
                doctor.status.success(),
                "{}",
                String::from_utf8_lossy(&doctor.stderr)
            );
            let stdout = String::from_utf8(doctor.stdout)?;
            assert_eq!(stdout.contains("stale_evidence"), stale, "{stdout}");
            let command = stdout
                .lines()
                .find(|line| line.starts_with("- Safe next action:"))
                .and_then(|line| line.split('`').nth(1))
                .ok_or("doctor emitted no packet refresh command")?;
            assert_eq!(
                command,
                format!("ripr first-pr --root '{}' --head HEAD", selected.display())
            );
            let recommended = stdout
                .lines()
                .find_map(|line| line.strip_prefix("- Recommended first command: "))
                .ok_or("doctor emitted no recommended check")?;
            assert_eq!(
                recommended,
                format!("ripr check --root '{}'", selected.display())
            );
            let after = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                after, before,
                "diagnosis must preserve both repositories' packets"
            );
        }
    }
    fs::remove_dir_all(base)?;
    Ok(())
}

/// Missing directories retain their diagnosis/recovery contract; physical
/// binding of existing roots must not create a directory or packet.
#[cfg(unix)]
#[test]
fn doctor_packet_refresh_missing_root_keeps_recovery_nonwriting() -> Result<(), Box<dyn Error>> {
    let base = replay::unique_temp_dir("doctor-refresh-missing-root")?;
    let missing = base.join("missing repository");
    let doctor = replay::ripr(&base, &["doctor", "--root", &missing.to_string_lossy()])?;
    // #5102: the diagnosis completed with findings, so it exits 3, not 2.
    assert_eq!(doctor.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&doctor.stderr).contains("doctor found issues"));
    let stdout = String::from_utf8(doctor.stdout)?;
    assert!(!stdout.contains("--head HEAD` refreshes it"), "{stdout}");
    // Compact failing doctor names the recovery on the fail/Fix/recommended
    // lines and omits the missing-packet dump (#8168).
    assert!(
        stdout.contains(
            "- Recommended first command: The selected root does not exist; rerun with `--root <path>` naming an existing repository directory"
        ),
        "{stdout}"
    );
    assert!(
        !stdout.contains("Recommended first command: ripr check --root"),
        "{stdout}"
    );
    assert!(!missing.exists());
    assert!(!base.join("target").exists());
    fs::remove_dir_all(base)?;
    Ok(())
}

/// A missing selected directory must not turn into an existing lexical decoy
/// merely because physical resolution could not finish.
#[cfg(unix)]
#[test]
fn doctor_packet_refresh_missing_alias_never_selects_decoy() -> Result<(), Box<dyn Error>> {
    let base = replay::unique_temp_dir("doctor-refresh-missing-alias")?;
    let parent = base.join("selected parent");
    let physical = base.join("physical parent");
    let decoy = parent.join("repo");
    replay::write_pr_fixture(&decoy)?;
    fs::create_dir_all(physical.join("child"))?;
    std::os::unix::fs::symlink(physical.join("child"), parent.join("link"))?;
    let relative = std::path::Path::new("link/../repo");
    let absolute = parent.join(relative);
    assert!(!absolute.exists());
    assert!(!physical.join("repo").exists());
    let reports = decoy.join("target/ripr/reports");
    fs::create_dir_all(&reports)?;
    let json = reports.join("start-here.json");
    let markdown = reports.join("start-here.md");
    fs::write(
        &json,
        serde_json::to_vec(&serde_json::json!({
            "ripr_version": env!("CARGO_PKG_VERSION")
        }))?,
    )?;
    fs::write(&markdown, "# decoy packet must remain untouched\n")?;
    let before = [fs::read(&json)?, fs::read(&markdown)?];
    for spelling in [relative, absolute.as_path()] {
        let doctor = replay::ripr(&parent, &["doctor", "--root", &spelling.to_string_lossy()])?;
        // #5102: the diagnosis completed with findings, so it exits 3, not 2.
        assert_eq!(doctor.status.code(), Some(3));
        let stdout = String::from_utf8(doctor.stdout)?;
        // Compact failing doctor names the recovery on the fail/Fix/recommended
        // lines and omits the missing-packet dump (#8168).
        assert!(!stdout.contains("--head HEAD` refreshes it"), "{stdout}");
        let command = stdout
            .lines()
            .find_map(|line| line.strip_prefix("- Recommended first command: "))
            .ok_or("doctor omitted its missing-root recovery")?;
        // #5252 item 3: the recommendation is the recovery action, which
        // renders no spelling at all, so neither the relative alias nor the
        // absolute form can resolve into the decoy here. The decoy-untouched
        // and non-creation assertions below stay the load-bearing half.
        assert_eq!(
            command,
            "The selected root does not exist; rerun with `--root <path>` naming an existing repository directory"
        );
        assert!(!absolute.exists());
        assert!(!physical.join("repo").exists());
        assert_eq!([fs::read(&json)?, fs::read(&markdown)?], before);
    }
    fs::remove_dir_all(base)?;
    Ok(())
}

/// A UTF-8 alias may select a directory whose actual name is not UTF-8.
/// Never promote a lossy replacement-character display into a runnable command.
#[cfg(unix)]
#[test]
fn doctor_packet_refresh_uses_lossless_alias_for_non_utf8_physical_root()
-> Result<(), Box<dyn Error>> {
    use std::os::unix::ffi::OsStringExt;
    let base = replay::unique_temp_dir("doctor-refresh-non-utf8-root")?;
    let parent = base.join("selected parent");
    let physical = base.join(std::ffi::OsString::from_vec(
        b"physical \xff parent".to_vec(),
    ));
    let repo = physical.join("repo");
    replay::write_pr_fixture(&repo)?;
    fs::create_dir_all(&parent)?;
    fs::create_dir_all(physical.join("child"))?;
    std::os::unix::fs::symlink(physical.join("child"), parent.join("link"))?;
    let relative = std::path::Path::new("link/../repo");
    let absolute = parent.join(relative);
    assert!(absolute.to_str().is_some());
    assert!(absolute.canonicalize()?.to_str().is_none());
    assert_eq!(absolute.canonicalize()?, repo.canonicalize()?);
    let reports = repo.join("target/ripr/reports");
    fs::create_dir_all(&reports)?;
    let json = reports.join("start-here.json");
    let markdown = reports.join("start-here.md");
    fs::write(
        &json,
        serde_json::to_vec(&serde_json::json!({
            "ripr_version": env!("CARGO_PKG_VERSION")
        }))?,
    )?;
    fs::write(&markdown, "# retained fixture packet\n")?;
    let before = [fs::read(&json)?, fs::read(&markdown)?];
    for spelling in [relative, absolute.as_path()] {
        let doctor = replay::ripr(&parent, &["doctor", "--root", &spelling.to_string_lossy()])?;
        assert!(
            doctor.status.success(),
            "{}",
            String::from_utf8_lossy(&doctor.stderr)
        );
        let stdout = String::from_utf8(doctor.stdout)?;
        let refresh = stdout
            .lines()
            .find(|line| line.starts_with("- Safe next action:"))
            .and_then(|line| line.split('`').nth(1))
            .ok_or("doctor emitted no lossless packet refresh")?;
        let check = stdout
            .lines()
            .find_map(|line| line.strip_prefix("- Recommended first command: "))
            .ok_or("doctor emitted no lossless recommended check")?;
        assert_eq!(
            refresh,
            format!("ripr first-pr --root '{}' --head HEAD", absolute.display())
        );
        assert_eq!(check, format!("ripr check --root '{}'", absolute.display()));
        assert!(!refresh.contains('\u{fffd}') && !check.contains('\u{fffd}'));
    }
    // Here both the physical spelling and the absolute original spelling
    // contain invalid UTF-8. Under-emit instead of inventing a lossy route.
    let unavailable = replay::ripr(&physical, &["doctor", "--root", "repo"])?;
    assert!(unavailable.status.success());
    let stdout = String::from_utf8(unavailable.stdout)?;
    assert!(
        stdout.contains("cannot be represented losslessly"),
        "{stdout}"
    );
    assert!(
        stdout.contains("- Safe next action: refresh unavailable"),
        "{stdout}"
    );
    assert!(
        stdout.contains("- Recommended first command unavailable:"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("- Recommended first command: "),
        "{stdout}"
    );
    assert!(!stdout.contains("--head HEAD` refreshes it"), "{stdout}");
    assert_eq!([fs::read(&json)?, fs::read(&markdown)?], before);
    fs::remove_file(&markdown)?;
    let no_packet = replay::ripr(&physical, &["doctor", "--root", "repo"])?;
    assert!(no_packet.status.success());
    let stdout = String::from_utf8(no_packet.stdout)?;
    // Compact omits the missing-packet dump; the lossless refusal still
    // names the next action (#8168).
    assert!(
        stdout.contains("- Safe next action: selected root cannot be represented losslessly"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("run the recommended first command below"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("- Recommended first command: "),
        "{stdout}"
    );
    assert_eq!(fs::read(&json)?, before[0]);
    assert!(!markdown.exists());
    fs::remove_dir_all(base)?;
    Ok(())
}

/// #3948/#4287: the artifact regeneration commands `first-pr` renders for
/// repository A, pasted into Bash from an unrelated directory B, read and
/// write A. `first-action`, `review-comments`, `agent packet`, `gate
/// evaluate`, `reports gap-ledger` and the `ripr check > FILE` redirects
/// resolve their paths against the working directory, so a command that
/// bound `--root` but kept a relative path would read or write under B. B
/// holds a decoy checkout under the same relative name, as in the status
/// replay above.
#[cfg(unix)]
#[test]
fn generated_first_pr_artifact_commands_run_from_a_foreign_working_directory()
-> Result<(), Box<dyn Error>> {
    for tool in ["bash", "git"] {
        if !replay::tool_available(tool) {
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                return Err(format!("`{tool}` is not on PATH under GitHub Actions").into());
            }
            eprintln!(
                "SKIPPED generated_first_pr_artifact_commands_run_from_a_foreign_working_directory: `{tool}` is not on PATH"
            );
            return Ok(());
        }
    }
    let base = replay::unique_temp_dir("first-pr-foreign-cwd")?;
    let parent = base.join("sélected parent");
    let repo = parent.join("repo root");
    let foreign = base.join("foreign cwd");
    let decoy = foreign.join("repo root");
    replay::write_pr_fixture(&repo)?;
    replay::write_pr_fixture(&decoy)?;
    let reports = repo.join("target/ripr/reports");
    fs::create_dir_all(&reports)?;
    fs::create_dir_all(repo.join("target/ripr/workflow"))?;

    // first-pr runs from A's parent with a relative --root, the way a user in
    // a monorepo parent directory selects the repository.
    let first_pr = |extra: &[&str]| -> Result<serde_json::Value, Box<dyn Error>> {
        let mut args = vec![
            "first-pr",
            "--root",
            "repo root",
            "--base",
            "origin/trunk",
            "--head",
            "HEAD",
        ];
        args.extend_from_slice(extra);
        let output = replay::ripr(&parent, &args)?;
        assert!(
            output.status.success(),
            "first-pr failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_str(&fs::read_to_string(
            reports.join("start-here.json"),
        )?)?)
    };
    // Paste one generated command from B; it must succeed, and nothing may
    // land under B or its decoy.
    let paste = |label: &str, command: &str| -> Result<(), Box<dyn Error>> {
        let run = replay::bash(&foreign, command, &[])?;
        assert!(
            run.status.success(),
            "`{label}` command failed from a foreign directory: {command}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(
            !foreign.join("target").exists() && !decoy.join("target").exists(),
            "`{label}` command read or wrote under the paste directory: {command}"
        );
        Ok(())
    };
    let artifact_command = |packet: &serde_json::Value,
                            id: &str|
     -> Result<(String, std::path::PathBuf), Box<dyn Error>> {
        let artifact = packet["artifacts"]
            .as_array()
            .and_then(|artifacts| artifacts.iter().find(|artifact| artifact["id"] == id))
            .ok_or_else(|| format!("start-here packet has no `{id}` artifact: {packet}"))?;
        let command = artifact["regeneration_command"]
            .as_str()
            .ok_or_else(|| format!("`{id}` has no regeneration command: {artifact}"))?;
        let path = artifact["path"]
            .as_str()
            .ok_or_else(|| format!("`{id}` has no path: {artifact}"))?;
        Ok((command.to_string(), repo.join(path)))
    };

    // Phase 1: the check-output artifact and the blocked-ledger recovery.
    // A Rust check output yields a blocked ledger, whose recovery compound
    // regenerates the repo-exposure input and the ledger.
    let check_output = "target/ripr/reports/check.json";
    let seed = replay::ripr(
        &repo,
        &["check", "--root", ".", "--base", "origin/trunk", "--json"],
    )?;
    fs::write(repo.join(check_output), &seed.stdout)?;
    let packet = first_pr(&["--check-output", check_output])?;
    let (command, artifact) = artifact_command(&packet, "check_output")?;
    fs::remove_file(&artifact)?;
    paste("check_output", &command)?;
    assert!(
        artifact.is_file(),
        "{command} did not write A's check output"
    );
    assert_eq!(packet["status"], "blocked", "{packet}");
    let recovery = packet["selected"]["next_command"]
        .as_str()
        .ok_or("blocked selection carries no next command")?;
    let ledger = reports.join("gap-decision-ledger.json");
    fs::remove_file(&ledger)?;
    paste("blocked gap ledger", recovery)?;
    assert!(ledger.is_file(), "{recovery} did not write A's gap ledger");
    assert!(reports.join("repo-exposure.json").is_file());

    // Phase 2: with a checked PR-local ledger whose top gap is agent-packet
    // eligible, first-pr selects that gap and renders the first-action,
    // review-comments, agent-packet and gate commands. Each is pasted from B
    // after A's copy is removed.
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../fixtures/first_successful_pr/python-preview-gap/inputs/reports/gap-decision-ledger.json",
        ),
        &ledger,
    )?;
    let packet = first_pr(&[])?;
    assert_eq!(packet["status"], "actionable", "{packet}");
    assert!(
        packet["selected"]["agent_packet_command"].is_string(),
        "the top gap must carry an agent packet command: {packet}"
    );
    for id in [
        "first_action",
        "review_comments",
        "agent_packet",
        "gate_decision",
    ] {
        let (command, artifact) = artifact_command(&packet, id)?;
        if artifact.exists() {
            fs::remove_file(&artifact)?;
        }
        paste(id, &command)?;
        assert!(
            artifact.is_file(),
            "`{id}` command did not write A's artifact: {command}"
        );
    }

    // Consumption: first-pr in A sees every regenerated artifact.
    let after = first_pr(&[])?;
    for artifact in after["artifacts"]
        .as_array()
        .ok_or("start-here packet has no artifacts")?
    {
        assert_eq!(artifact["status"], "present", "{artifact}");
    }

    fs::remove_dir_all(base)?;
    Ok(())
}

/// The preflight recovery commands `first-pr` prints when the head or base ref
/// is missing or the range has no diff interpolate the selected root and the
/// refs. A root with spaces or non-ASCII characters must stay one argument,
/// and a ref such as `topic;touch injected-marker` must stay one argument
/// instead of running a second command. Each printed rerun command is pasted
/// from a foreign directory holding a decoy checkout under the same relative
/// name: it must name the selected root and leave nothing behind in the
/// foreign directory, the decoy or a marker file.
#[cfg(unix)]
#[test]
fn generated_first_pr_preflight_recovery_commands_quote_root_and_refs() -> Result<(), Box<dyn Error>>
{
    for tool in ["bash", "git"] {
        if !replay::tool_available(tool) {
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                return Err(format!("`{tool}` is not on PATH under GitHub Actions").into());
            }
            eprintln!(
                "SKIPPED generated_first_pr_preflight_recovery_commands_quote_root_and_refs: `{tool}` is not on PATH"
            );
            return Ok(());
        }
    }
    let base = replay::unique_temp_dir("first-pr-preflight-quoting")?;
    let parent = base.join("sélected parent");
    let repo = parent.join("repo root");
    let foreign = base.join("foreign cwd");
    let decoy = foreign.join("repo root");
    replay::write_pr_fixture(&repo)?;
    replay::write_pr_fixture(&decoy)?;
    let canonical_repo = repo.canonicalize()?;
    // A local `origin` lets the fetch half of the missing-base hint execute.
    replay::git(&repo, &["remote", "add", "origin", "."])?;
    let unsafe_ref = "topic;touch injected-marker";

    // (label, base ref, head ref, id of the preflight check that recovers)
    let cases = [
        ("missing head", "origin/trunk", unsafe_ref, "git_head"),
        (
            "missing base",
            "origin/x;touch injected-marker",
            "HEAD",
            "git_base",
        ),
        (
            "option-shaped base",
            "origin/--upload-pack=touch injected-marker",
            "HEAD",
            "git_base",
        ),
        ("no diff", "HEAD", "HEAD", "git_diff"),
    ];
    for (label, base_ref, head_ref, check_id) in cases {
        let output = replay::ripr(
            &parent,
            &[
                "first-pr",
                "--root",
                "repo root",
                "--base",
                base_ref,
                "--head",
                head_ref,
            ],
        )?;
        assert!(
            output.status.success(),
            "{label}: first-pr failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let packet: serde_json::Value = serde_json::from_str(&fs::read_to_string(
            repo.join("target/ripr/reports/start-here.json"),
        )?)?;
        let recovery = recovery_texts(&packet, check_id);
        assert!(
            !recovery.is_empty(),
            "{label}: no `{check_id}` recovery command in {packet}"
        );
        for text in recovery {
            // The refs and root must appear shell-quoted wherever they occur.
            for hostile in [
                unsafe_ref,
                "origin/x;touch injected-marker",
                "origin/--upload-pack=touch injected-marker",
            ] {
                for (at, _) in text.match_indices(hostile) {
                    let quoted = recovery_ref_occurrence_is_shell_quoted(&text, at, hostile.len());
                    let in_prose =
                        text[..at].ends_with('`') && text[at + hostile.len()..].starts_with('`');
                    assert!(quoted || in_prose, "{label}: unquoted ref in `{text}`");
                }
            }
            // The fetch half of a missing-base hint is a real git command:
            // run it against a local `origin` so an option-shaped branch that
            // git would read as `--upload-pack` executes the marker command.
            if let Some(fetch) = text
                .split("; then rerun")
                .next()
                .filter(|fetch| fetch.starts_with("git fetch origin "))
            {
                assert!(
                    fetch.starts_with("git fetch origin -- "),
                    "{label}: fetch does not end option parsing before the branch: {fetch}"
                );
                replay::bash(
                    &foreign,
                    &format!(
                        "git -C '{}' {}",
                        canonical_repo.display(),
                        &fetch["git ".len()..]
                    ),
                    &[],
                )?;
                assert!(
                    !foreign.join("injected-marker").exists()
                        && !repo.join("injected-marker").exists(),
                    "{label}: fetch hint executed an injected command: {fetch}"
                );
            }
            let rerun = text
                .split('`')
                .find(|segment| segment.starts_with("ripr first-pr "))
                .ok_or_else(|| format!("{label}: no `ripr first-pr` rerun in `{text}`"))?;
            assert!(
                rerun.contains(&format!("--root '{}'", canonical_repo.display())),
                "{label}: rerun does not bind the quoted root: {rerun}"
            );
            let run = replay::bash(&foreign, rerun, &[])?;
            let stdout = String::from_utf8_lossy(&run.stdout);
            let stderr = String::from_utf8_lossy(&run.stderr);
            assert!(
                run.status.success(),
                "{label}: rerun failed from a foreign directory: {rerun}\nstdout={stdout}\nstderr={stderr}"
            );
            assert!(
                stdout.contains(&canonical_repo.display().to_string()),
                "{label}: rerun did not analyze the selected root: {stdout}"
            );
            assert!(
                !stderr.contains("command not found"),
                "{label}: rerun executed an injected command: {stderr}"
            );
            assert!(
                !foreign.join("injected-marker").exists()
                    && !foreign.join("target").exists()
                    && !decoy.join("target").exists()
                    && !decoy.join("injected-marker").exists()
                    && !repo.join("injected-marker").exists(),
                "{label}: rerun read or wrote outside the selected root: {rerun}"
            );
        }
    }

    // Both refs still resolve, but `git diff` fails once the head tree object is
    // missing (#5339).
    let tree = fixture_git_output(&repo, &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_string();
    assert!(
        tree.len() == 40 && tree.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "fixture did not produce a SHA-1 tree: {tree:?}"
    );
    let object = repo.join(".git/objects").join(&tree[..2]).join(&tree[2..]);
    fs::remove_file(&object)?;
    let label = "diff inspect failure";
    let output = replay::ripr(
        &parent,
        &[
            "first-pr",
            "--root",
            "repo root",
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
        ],
    )?;
    assert!(
        output.status.success(),
        "{label}: first-pr failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let packet: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        repo.join("target/ripr/reports/start-here.json"),
    )?)?;
    let diff_check = packet["preflight"]["checks"]
        .as_array()
        .and_then(|checks| {
            checks
                .iter()
                .find(|check| check.get("id").and_then(|id| id.as_str()) == Some("git_diff"))
        })
        .ok_or_else(|| format!("{label}: no git_diff preflight check in {packet}"))?;
    assert_eq!(
        diff_check["status"].as_str(),
        Some("needs_attention"),
        "{label}: {packet}"
    );
    assert!(
        diff_check["message"]
            .as_str()
            .is_some_and(|message| message.contains("Could not inspect diff range")),
        "{label}: {packet}"
    );
    let recovery = recovery_texts(&packet, "git_diff");
    assert!(
        !recovery.is_empty(),
        "{label}: no `git_diff` recovery command in {packet}"
    );
    for text in recovery {
        let rerun = text
            .split('`')
            .find(|segment| segment.starts_with("ripr first-pr "))
            .ok_or_else(|| format!("{label}: no `ripr first-pr` rerun in `{text}`"))?;
        assert!(
            rerun.contains(&format!("--root '{}'", canonical_repo.display())),
            "{label}: rerun does not bind the quoted root: {rerun}"
        );
        let run = replay::bash(&foreign, rerun, &[])?;
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            run.status.success(),
            "{label}: rerun failed from a foreign directory: {rerun}\nstdout={stdout}\nstderr={stderr}"
        );
        assert!(
            stdout.contains(&canonical_repo.display().to_string()),
            "{label}: rerun did not analyze the selected root: {stdout}"
        );
        assert!(
            !foreign.join("target").exists() && !decoy.join("target").exists(),
            "{label}: rerun read or wrote outside the selected root: {rerun}"
        );
    }

    fs::remove_dir_all(base)?;
    Ok(())
}

/// Whether one occurrence of a hostile token is a shell-quoted argument, or
/// sits inside a closed quoted argument such as a fetch refspec.
///
/// Quote-count parity (`matches('\'').count() % 2`) is not enough: an odd or
/// unterminated `'` before an unquoted token, plus any later quote, would
/// pass that check. Walk quote state so a closer of `'a '` cannot reopen
/// around a following unquoted token. The opening quote must be a delimiter
/// (start of the string or after whitespace), so unquoted contractions such
/// as `don't` do not open a quote. Once open, POSIX single quotes close on
/// the next `'`, even when a word character follows (`'a'{ref}` is quoted
/// `a` concatenated with an unquoted ref). A closer followed by `; then`
/// remains a closer. Adjacent `'token'` wrapping is the same walk: the
/// opener sits after whitespace, so quote-state already accepts it. Quotes
/// merely touching both ends of a token (`'a'{ref}''`) are not enough.
fn recovery_ref_occurrence_is_shell_quoted(text: &str, at: usize, len: usize) -> bool {
    inside_closed_single_quoted_argument(text, at, at + len)
}

fn inside_closed_single_quoted_argument(text: &str, start: usize, end: usize) -> bool {
    let mut in_quote = false;
    for (i, ch) in text.char_indices() {
        if i >= start {
            break;
        }
        if ch != '\'' {
            continue;
        }
        if in_quote {
            in_quote = false;
        } else if i == 0 || text[..i].ends_with(char::is_whitespace) {
            in_quote = true;
        }
    }
    in_quote && text.get(end..).is_some_and(|rest| rest.contains('\''))
}

#[test]
fn recovery_ref_quoting_accepts_adjacent_and_refspec_forms() {
    let hostile = "topic;touch injected-marker";
    assert!(recovery_ref_occurrence_is_shell_quoted(
        &format!("ripr first-pr --base '{hostile}' --head HEAD"),
        "ripr first-pr --base '".len(),
        hostile.len(),
    ));
    let refspec =
        format!("git fetch origin -- '+refs/heads/{hostile}:refs/remotes/origin/{hostile}'");
    let at = refspec.find(hostile);
    assert!(at.is_some(), "refspec must carry the branch: {refspec}");
    let Some(at) = at else {
        return;
    };
    assert!(recovery_ref_occurrence_is_shell_quoted(
        &refspec,
        at,
        hostile.len(),
    ));
    // Producer missing-base `next_command`: the closer is followed by `; then`.
    let missing_base = "origin/x;touch injected-marker";
    let compound = format!(
        "git fetch origin -- '+refs/heads/x;touch injected-marker:refs/remotes/{missing_base}'; then rerun `ripr first-pr --base '{missing_base}'`"
    );
    for (at, _) in compound.match_indices(missing_base) {
        assert!(
            recovery_ref_occurrence_is_shell_quoted(&compound, at, missing_base.len()),
            "producer compound quoting must hold at {at}: {compound}"
        );
    }
}

#[test]
fn odd_unterminated_quote_before_unquoted_hostile_ref_is_not_quoted() {
    let hostile = "topic;touch injected-marker";
    // Odd apostrophe before the ref plus a later quote: the discarded
    // `matches('\'').count() % 2` check would accept this.
    let odd = format!("don't fetch {hostile} 'later'");
    let at = odd.find(hostile);
    assert!(at.is_some(), "fixture must carry the ref: {odd}");
    let Some(at) = at else {
        return;
    };
    assert_eq!(odd[..at].matches('\'').count() % 2, 1);
    assert!(odd[at + hostile.len()..].contains('\''));
    assert!(
        !recovery_ref_occurrence_is_shell_quoted(&odd, at, hostile.len()),
        "odd quote before an unquoted ref must fail: {odd}"
    );

    let unterminated = format!("git fetch origin -- ' then {hostile}");
    let at = unterminated.find(hostile);
    assert!(at.is_some(), "fixture must carry the ref: {unterminated}");
    let Some(at) = at else {
        return;
    };
    assert!(
        !recovery_ref_occurrence_is_shell_quoted(&unterminated, at, hostile.len()),
        "unterminated quote before an unquoted ref must fail: {unterminated}"
    );

    // Closed quotes on both sides of an unquoted ref: last-quote-wins would
    // treat the closer of `'a '` as an opener because a space precedes it.
    let separated = format!("git fetch origin -- 'a ' {hostile} ' b'");
    let at = separated.find(hostile);
    assert!(at.is_some(), "fixture must carry the ref: {separated}");
    let Some(at) = at else {
        return;
    };
    assert_eq!(separated[..at].matches('\'').count() % 2, 0);
    assert!(
        !recovery_ref_occurrence_is_shell_quoted(&separated, at, hostile.len()),
        "unquoted ref between two quoted fragments must fail: {separated}"
    );

    // POSIX: a single quote closes even when a word character follows.
    // `'a'{ref}` is quoted `a` concatenated with an unquoted ref. An
    // in-word apostrophe heuristic would keep the quote open and accept it.
    let glued = format!("git fetch origin -- 'a'{hostile} ' b'");
    let at = glued.find(hostile);
    assert!(at.is_some(), "fixture must carry the ref: {glued}");
    let Some(at) = at else {
        return;
    };
    assert!(
        !recovery_ref_occurrence_is_shell_quoted(&glued, at, hostile.len()),
        "unquoted ref glued to a closed quote must fail: {glued}"
    );

    // Quotes touching both ends are not enough: `'a'{ref}''` still has the
    // ref unquoted after `'a'` closes. An adjacent-quote shortcut would
    // accept this.
    let glued_empty = format!("git fetch origin -- 'a'{hostile}''");
    let at = glued_empty.find(hostile);
    assert!(at.is_some(), "fixture must carry the ref: {glued_empty}");
    let Some(at) = at else {
        return;
    };
    assert!(
        !recovery_ref_occurrence_is_shell_quoted(&glued_empty, at, hostile.len()),
        "unquoted ref between a closer and an empty quote must fail: {glued_empty}"
    );
}

/// Every `next_command` string the packet carries on the named check.
/// Unix-only: the sole caller is the `#[cfg(unix)]` preflight-quoting test,
/// so an ungated helper is dead code on Windows and fails `-D warnings`.
#[cfg(unix)]
fn recovery_texts(packet: &serde_json::Value, check_id: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![packet];
    while let Some(value) = stack.pop() {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("id").and_then(|id| id.as_str()) == Some(check_id)
                    && let Some(command) = map.get("next_command").and_then(|c| c.as_str())
                {
                    found.push(command.to_string());
                }
                stack.extend(map.values());
            }
            serde_json::Value::Array(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
    found
}

/// Review comments and `::warning` annotations are placed on the PR head's
/// lines, so the generated workflow must analyze the PR head. On a
/// `pull_request` event `actions/checkout` defaults to `refs/pull/N/merge`;
/// when the base branch has moved lines in a changed file, merge-commit line
/// numbers point past the change on the head, and GitHub rejects the whole
/// review when a comment line falls outside the PR diff.
///
/// The fixture's base branch adds four doc lines above the changed `>=`
/// after the PR branched: the change is line 4 on the head and line 8 on the
/// merge commit. The test checks out what the generated checkout step names
/// (the merge commit when it names no ref), replays the diff, guidance, and
/// annotation steps, and requires every placement to land on the changed
/// line of the PR head.
#[cfg(unix)]
#[test]
fn generated_workflow_places_findings_on_pr_head_lines_when_base_moved()
-> Result<(), Box<dyn Error>> {
    for tool in ["bash", "git", "jq"] {
        if !replay::tool_available(tool) {
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                return Err(format!("`{tool}` is not on PATH under GitHub Actions").into());
            }
            eprintln!(
                "SKIPPED generated_workflow_places_findings_on_pr_head_lines_when_base_moved: `{tool}` is not on PATH"
            );
            return Ok(());
        }
    }
    let rev_parse = |root: &std::path::Path, rev: &str| {
        fixture_git_output(root, &["rev-parse", "--verify", rev]).map(|sha| sha.trim().to_string())
    };

    let base = replay::unique_temp_dir("head-lines")?;
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
    let head_sha = rev_parse(&root, "HEAD")?;

    // The base branch moves on: four doc lines above the threshold check.
    replay::git(&root, &["checkout", "-q", "trunk"])?;
    let lib = fs::read_to_string(root.join("src/lib.rs"))?;
    fs::write(
        root.join("src/lib.rs"),
        format!(
            "//! Pricing.\n//! Discounts apply above the threshold.\n//! Amounts are in cents.\n\n{lib}"
        ),
    )?;
    replay::git(&root, &["commit", "-q", "-a", "-m", "document pricing"])?;
    replay::git(&root, &["update-ref", "refs/remotes/origin/trunk", "HEAD"])?;

    // What GitHub builds as refs/pull/N/merge: base first, PR head second.
    replay::git(&root, &["checkout", "-q", "--detach", "trunk"])?;
    replay::git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge feature into trunk",
            "feature",
        ],
    )?;
    let merge_sha = rev_parse(&root, "HEAD")?;

    let head_lib = fixture_git_output(&root, &["show", &format!("{head_sha}:src/lib.rs")])?;
    let merge_lib = fs::read_to_string(root.join("src/lib.rs"))?;
    let changed_line = |text: &str| {
        text.lines()
            .position(|line| line.contains("amount >= DISCOUNT_THRESHOLD"))
            .map(|index| index + 1)
    };
    // Precondition: the fixture really moves the changed line.
    assert_eq!(changed_line(&head_lib), Some(4), "{head_lib}");
    assert_eq!(changed_line(&merge_lib), Some(8), "{merge_lib}");

    // Resolve the generated checkout step the way actions/checkout does for
    // a pull_request event: no `ref` means the merge commit.
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let checkout = workflow
        .split("      - uses: actions/checkout@")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .ok_or("generated workflow has no actions/checkout step")?;
    let checkout_ref = checkout
        .lines()
        .find_map(|line| line.trim().strip_prefix("ref:"))
        // A trailing YAML comment is not part of the value.
        .map(|value| {
            value
                .split_once(" # ")
                .map_or(value, |(value, _)| value)
                .trim()
        });
    let checkout_sha = match checkout_ref {
        None => merge_sha.clone(),
        Some("${{ github.event.pull_request.head.sha || github.sha }}") => head_sha.clone(),
        Some(other) => return Err(format!("test does not model checkout ref `{other}`").into()),
    };
    replay::git(&root, &["checkout", "-q", "--detach", &checkout_sha])?;

    let wanted = ["Run RIPR"];
    let steps = replay::parse_steps(&workflow)
        .into_iter()
        .filter(|step| wanted.contains(&step.name.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(steps.len(), wanted.len(), "template step names changed");
    let runs = replay::run_workflow(&root, &base, &steps)?;
    for stage in [
        "Capture pull request diff",
        "Run RIPR PR guidance report",
        "Emit RIPR PR guidance annotations",
    ] {
        assert!(
            runs.iter().any(|run| run.name == stage),
            "`{stage}` did not run"
        );
    }
    for run in &runs {
        assert_eq!(
            run.exit_code,
            Some(0),
            "{} failed:\n{}",
            run.name,
            run.output
        );
    }

    let on_head_change = |path: &str, line: u64| {
        path == "src/lib.rs" && usize::try_from(line).ok() == changed_line(&head_lib)
    };
    let comments: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        root.join("target/ripr/review/comments.json"),
    )?)?;
    let placements = comments["comments"]
        .as_array()
        .ok_or("comments.json has no comments array")?
        .iter()
        .filter_map(|comment| {
            Some((
                comment.pointer("/placement/path")?.as_str()?.to_string(),
                comment.pointer("/placement/line")?.as_u64()?,
            ))
        })
        .collect::<Vec<_>>();
    // Precondition: the fixture's gap produced a line-placed card.
    assert!(
        !placements.is_empty(),
        "no line-placed review card: {comments}"
    );
    for (path, line) in &placements {
        assert!(
            on_head_change(path, *line),
            "review card placed at {path}:{line}; the PR head changed src/lib.rs:4"
        );
    }

    let annotations = runs
        .iter()
        .find(|run| run.name == "Emit RIPR PR guidance annotations")
        .ok_or("annotation step did not run")?
        .output
        .lines()
        .filter_map(|line| line.strip_prefix("::warning file="))
        .map(|rest| {
            let (path, rest) = rest.split_once(",line=").unwrap_or((rest, ""));
            let line = rest
                .split([',', ':'])
                .next()
                .unwrap_or("")
                .parse::<u64>()
                .unwrap_or(0);
            (path.to_string(), line)
        })
        .collect::<Vec<_>>();
    assert!(!annotations.is_empty(), "no annotation was emitted");
    for (path, line) in &annotations {
        assert!(
            on_head_change(path, *line),
            "annotation placed at {path}:{line}; the PR head changed src/lib.rs:4"
        );
    }

    fs::remove_dir_all(base)?;
    Ok(())
}

/// GitHub gives Dependabot-triggered `pull_request` runs a read-only token
/// whatever the workflow's `permissions:` block grants. The generated plan
/// step must then not claim write permission, or the plan marks inline
/// comments safe and the publish step fails with 403. A run by any other
/// actor keeps the same-repo plan publishable (the control).
#[cfg(unix)]
#[test]
fn generated_comment_plan_withholds_write_permission_for_dependabot() -> Result<(), Box<dyn Error>>
{
    for tool in ["bash", "git", "jq"] {
        if !replay::tool_available(tool) {
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                return Err(format!("`{tool}` is not on PATH under GitHub Actions").into());
            }
            eprintln!(
                "SKIPPED generated_comment_plan_withholds_write_permission_for_dependabot: `{tool}` is not on PATH"
            );
            return Ok(());
        }
    }
    let plan_for = |overrides: &[(&str, &str)]| -> Result<serde_json::Value, Box<dyn Error>> {
        let base = replay::unique_temp_dir("dependabot-plan")?;
        let root = base.join("repo");
        replay::write_pr_fixture(&root)?;
        let init = replay::ripr(&root, &["init", "--root", ".", "--ci", "github"])?;
        assert!(
            init.status.success(),
            "ripr init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
        let wanted = ["Run RIPR"];
        let steps = replay::parse_steps(&workflow)
            .into_iter()
            .filter(|step| wanted.contains(&step.name.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(steps.len(), wanted.len(), "template step names changed");
        let mut env = vec![("RIPR_COMMENT_MODE", "inline")];
        env.extend_from_slice(overrides);
        let runs = replay::run_workflow_with_env(&root, &base, &steps, &env)?;
        for stage in [
            "Capture pull request diff",
            "Run RIPR PR guidance report",
            "Plan RIPR inline comments",
        ] {
            assert!(
                runs.iter().any(|run| run.name == stage),
                "`{stage}` was skipped"
            );
        }
        for run in &runs {
            assert_eq!(
                run.exit_code,
                Some(0),
                "{} failed:\n{}",
                run.name,
                run.output
            );
        }
        let plan = serde_json::from_str(&fs::read_to_string(
            root.join("target/ripr/review/comment-publish-plan.json"),
        )?)?;
        fs::remove_dir_all(base)?;
        Ok(plan)
    };

    let control = plan_for(&[])?;
    assert_eq!(
        control.pointer("/summary/safe_to_publish"),
        Some(&serde_json::Value::Bool(true)),
        "control: a same-repo run by a user must stay publishable: {control}"
    );

    // Dependabot as the event actor, and a maintainer reopening a
    // Dependabot-authored PR (the actor is the maintainer).
    for (case, overrides) in [
        ("dependabot actor", [("GITHUB_ACTOR", "dependabot[bot]")]),
        (
            "dependabot-authored PR",
            [("RIPR_REPLAY_PR_AUTHOR", "dependabot[bot]")],
        ),
    ] {
        let plan = plan_for(&overrides)?;
        assert_eq!(
            plan.pointer("/summary/safe_to_publish"),
            Some(&serde_json::Value::Bool(false)),
            "{case}: the run has a read-only token: {plan}"
        );
        let reasons = plan["blocked"]
            .as_array()
            .ok_or("plan has no blocked array")?
            .iter()
            .filter_map(|blocked| blocked["blocked_reason"].as_str())
            .collect::<Vec<_>>();
        assert!(
            reasons.contains(&"missing_write_permission"),
            "{case}: blocked reasons: {reasons:?}"
        );
    }
    Ok(())
}

/// #11145: the generated `Explain failed SARIF upload` condition, evaluated by
/// the replay's own condition model, runs when either upload step failed and
/// not when both succeeded or were skipped; the evaluator rejects shapes it
/// does not model instead of guessing.
#[cfg(unix)]
#[test]
fn explain_failed_sarif_upload_condition_follows_the_upload_outcomes() -> Result<(), Box<dyn Error>>
{
    use std::collections::BTreeMap;
    let base = replay::unique_temp_dir("sarif-explain")?;
    let root = base.join("repo");
    replay::write_pr_fixture(&root)?;
    let init = replay::ripr(&root, &["init", "--root", ".", "--ci", "github"])?;
    assert!(init.status.success());
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let condition = replay::parse_steps(&workflow)
        .into_iter()
        .find(|step| step.name == "Explain failed SARIF upload")
        .and_then(|step| step.condition)
        .ok_or("no condition on the explain step")?;
    let holds = |findings: Option<&str>, seams: Option<&str>| {
        let mut env = BTreeMap::new();
        if let Some(outcome) = findings {
            env.insert(
                "REPLAY_STEP_OUTCOME_sarif_findings".to_string(),
                outcome.to_string(),
            );
        }
        if let Some(outcome) = seams {
            env.insert(
                "REPLAY_STEP_OUTCOME_sarif_seams".to_string(),
                outcome.to_string(),
            );
        }
        replay::condition_holds(&condition, &root, &env)
    };
    let verdicts = (
        holds(Some("failure"), Some("success"))?,
        holds(Some("success"), Some("failure"))?,
        holds(Some("failure"), Some("failure"))?,
        holds(Some("success"), Some("success"))?,
        holds(Some("skipped"), Some("skipped"))?,
        holds(None, None)?,
        holds(Some("cancelled"), Some("success"))?,
    );
    // Shapes the evaluator does not model are errors, not guesses.
    let nested = replay::condition_holds(
        "always() && ((steps.sarif_seams.outcome == 'failure'))",
        &root,
        &BTreeMap::new(),
    );
    let group_not_last = replay::condition_holds(
        "always() && (steps.sarif_seams.outcome == 'failure') && always()",
        &root,
        &BTreeMap::new(),
    );
    let _ = fs::remove_dir_all(&base);
    assert_eq!(
        verdicts,
        (true, true, true, false, false, false, false),
        "(findings failed, seams failed, both failed, both ok, both skipped, default, cancelled)"
    );
    assert!(nested.is_err(), "nested groups must be refused");
    assert!(
        group_not_last.is_err(),
        "a group that is not last must be refused"
    );
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
        pub(super) condition: Option<String>,
        pub(super) run: Option<String>,
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
    /// the tests never exercise at the boundary: a repair-ready gap. The PR
    /// targets `trunk`, not `main`, so a step that falls back to a
    /// hardcoded `origin/main` fails here instead of passing by accident.
    pub(super) fn write_pr_fixture(root: &Path) -> TestResult<()> {
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir_all(root.join("tests"))?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )?;
        fs::write(root.join("src/lib.rs"), LIB_BASE)?;
        fs::write(root.join("tests/pricing.rs"), TESTS)?;
        // Printed repair commands require the adopter's whole Cargo build
        // directory to be effectively ignored before the transaction starts.
        fs::write(root.join(".gitignore"), "/target/\n")?;
        git(root, &["init", "-q", "-b", "trunk"])?;
        git(root, &["add", "-A"])?;
        git(root, &["commit", "-q", "-m", "initial pricing crate"])?;
        git(root, &["update-ref", "refs/remotes/origin/trunk", "HEAD"])?;
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
        ripr_with_env(dir, args, &[], &[])
    }

    /// `ripr` with `removed` taken out of the inherited environment and
    /// `set` added, so a test does not depend on the runner's own `GITHUB_*`.
    pub(super) fn ripr_with_env(
        dir: &Path,
        args: &[&str],
        removed: &[&str],
        set: &[(&str, &str)],
    ) -> TestResult<Output> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command.args(args).current_dir(dir).stdin(Stdio::null());
        for name in removed {
            command.env_remove(name);
        }
        for (name, value) in set {
            command.env(name, value);
        }
        Ok(command.output()?)
    }

    fn path_with_ripr() -> TestResult<String> {
        let bin = Path::new(env!("CARGO_BIN_EXE_ripr"))
            .parent()
            .ok_or("ripr binary has no parent directory")?;
        let inherited = std::env::var("PATH").unwrap_or_default();
        Ok(format!("{}:{inherited}", bin.display()))
    }

    /// Run a script the way a GitHub-hosted Linux step does under the
    /// generated workflow's `defaults.run.shell: bash`
    /// (`bash --noprofile --norc -eo pipefail {0}`), with the freshly built
    /// `ripr` first on PATH.
    pub(super) fn bash(dir: &Path, script: &str, env: &[(String, String)]) -> TestResult<Output> {
        let mut command = Command::new("bash");
        command
            .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script])
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
            "github.base_ref" => "trunk",
            // A pull_request event carries base_ref, so the default-branch
            // fallback never applies in this replay.
            "github.base_ref || github.event.repository.default_branch" => "trunk",
            "github.event_name" => "pull_request",
            "github.repository" => "ripr-test/pricing",
            "github.event.number" | "github.event.pull_request.number" => "1",
            "github.event.pull_request.head.repo.full_name" => "ripr-test/pricing",
            "github.event.pull_request.head.sha" => head_sha,
            "github.token" => "replay-token-unused",
            // The replay skips the install and runs the ripr under test,
            // which is installed.
            "steps.install.outcome" => "success",
            "github.actor" | "github.event.pull_request.user.login" => "ripr-test-user",
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
        // The replay is a same-repository pull request by a human author.
        if matches!(
            text,
            "github.event.pull_request.head.repo.full_name" | "github.repository"
        ) {
            return Ok("ripr-test/repo".to_string());
        }
        if text == "github.actor" {
            return Ok("ripr-test-user".to_string());
        }
        // The replay skips every `uses:` step, the SARIF uploads included, so
        // by default no upload failed (#11145). A test sets the outcome with
        // `REPLAY_STEP_OUTCOME_<id>` to drive the failure path.
        if let Some(id) = text
            .strip_prefix("steps.")
            .and_then(|rest| rest.strip_suffix(".outcome"))
            .filter(|id| matches!(*id, "sarif_findings" | "sarif_seams"))
        {
            return Ok(env
                .get(&format!("REPLAY_STEP_OUTCOME_{id}"))
                .cloned()
                .unwrap_or_else(|| "success".to_string()));
        }
        Err(format!("replay does not model the condition operand `{text}`").into())
    }

    pub(super) fn condition_holds(
        condition: &str,
        root: &Path,
        env: &BTreeMap<String, String>,
    ) -> TestResult<bool> {
        // A conjunction whose last atom is one parenthesized disjunction:
        // `always() && (a == 'x' || b == 'x')`. Only that shape is modeled:
        // one group, last, with no nesting and no `&&` inside it.
        if let Some((head, group)) = condition.split_once("&& (") {
            let group = group
                .trim()
                .strip_suffix(')')
                .ok_or_else(|| format!("replay: group is not last in `{condition}`"))?;
            if group.contains(['(', ')']) || group.contains("&&") || head.contains("||") {
                return Err(
                    format!("replay models one flat `(a || b)` group, not `{condition}`").into(),
                );
            }
            if !condition_holds(head, root, env)? {
                return Ok(false);
            }
            for alternative in group.split("||") {
                if condition_holds(alternative, root, env)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
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
        run_workflow_with_env(root, base, steps, &[])
    }

    /// `run_workflow` with environment overrides that win over both the
    /// workflow-level defaults (so step conditions see them) and each
    /// step's own `env:` entries, as a repository variable or a different
    /// event actor would.
    pub(super) fn run_workflow_with_env(
        root: &Path,
        base: &Path,
        steps: &[Step],
        overrides: &[(&str, &str)],
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
        // `ripr reports ci-packet` reads the PR facts from the event, as the
        // retired steps read `github.event.*`. `RIPR_REPLAY_PR_AUTHOR` sets
        // the PR author a test needs; it is not a workflow variable.
        let author = overrides
            .iter()
            .find(|(name, _)| *name == "RIPR_REPLAY_PR_AUTHOR")
            .map_or("ripr-test-user", |(_, value)| *value);
        fs::write(
            &event,
            serde_json::json!({
                "number": 1,
                "repository": {"default_branch": "trunk"},
                "pull_request": {
                    "number": 1,
                    "labels": [],
                    "user": {"login": author},
                    "head": {"sha": head_sha, "repo": {"full_name": "ripr-test/pricing"}}
                }
            })
            .to_string(),
        )?;
        let mut env = BTreeMap::from([
            ("RIPR_UPLOAD_SARIF".to_string(), "true".to_string()),
            ("RIPR_GATE_MODE".to_string(), String::new()),
            ("RIPR_GATE_BASELINE".to_string(), String::new()),
            ("RIPR_COMMENT_MODE".to_string(), "off".to_string()),
            // The replay never reaches GitHub; the summary step has its own test.
            ("RIPR_SUMMARY_COMMENT".to_string(), "false".to_string()),
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
            ("GITHUB_BASE_REF".to_string(), "trunk".to_string()),
            ("GITHUB_HEAD_REF".to_string(), "feature".to_string()),
            (
                "GITHUB_REPOSITORY".to_string(),
                "ripr-test/pricing".to_string(),
            ),
            ("GITHUB_SHA".to_string(), head_sha.clone()),
            ("GITHUB_ACTOR".to_string(), "ripr-test-user".to_string()),
            ("GITHUB_WORKSPACE".to_string(), root.display().to_string()),
            ("RUNNER_TEMP".to_string(), base.display().to_string()),
        ]);
        for (name, value) in overrides {
            env.insert((*name).to_string(), (*value).to_string());
        }
        let mut runs = Vec::new();
        for step in steps {
            let Some(script) = &step.run else {
                continue;
            };
            // The replay runs the ripr under test, not a downloaded release.
            if step.name == "Install ripr" {
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
                if overrides.iter().any(|(overridden, _)| overridden == name) {
                    continue;
                }
                step_env.push((name.clone(), substitute(value, &head_sha)?));
            }
            let output = bash(root, &script, &step_env)?;
            let run = StepRun {
                name: step.name.clone(),
                exit_code: output.status.code(),
                output: format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            };
            let stages = if step.name == "Run RIPR" {
                stage_runs(&String::from_utf8_lossy(&output.stdout))
            } else {
                Vec::new()
            };
            runs.push(run);
            runs.extend(stages);
            for line in fs::read_to_string(&github_env)?.lines() {
                if let Some((name, value)) = line.split_once('=') {
                    env.insert(name.to_string(), value.to_string());
                }
            }
        }
        Ok(runs)
    }

    /// `ripr reports ci-packet` prints each retired workflow step as a log
    /// group, and names a failed step after its group. Split its stdout back
    /// into per-step runs so assertions keep naming the step they hold.
    fn stage_runs(stdout: &str) -> Vec<StepRun> {
        let mut stages: Vec<StepRun> = Vec::new();
        let mut open: Option<StepRun> = None;
        for line in stdout.lines() {
            if let Some(name) = line.strip_prefix("::group::") {
                open = Some(StepRun {
                    name: name.to_string(),
                    exit_code: Some(0),
                    output: String::new(),
                });
            } else if line == "::endgroup::" {
                stages.extend(open.take());
            } else if let Some(run) = open.as_mut() {
                run.output.push_str(line);
                run.output.push('\n');
            } else if let Some(stage) = stages.last_mut().filter(|stage| {
                line.starts_with(&format!("RIPR step \"{}\" failed", stage.name))
                    || line.starts_with(&format!("::error title={}::", stage.name))
            }) {
                stage.exit_code = Some(1);
                stage.output.push_str(line);
                stage.output.push('\n');
            }
        }
        stages.extend(open);
        stages
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

/// #4005: the generated "Capture pull request diff" step must use the pinned
/// diff contract (same presentation pins as the production loaders), resolve
/// and retain the exact base/head identities, record the patch digest, and
/// fail closed instead of handing RIPR an ambient-presentation patch.
#[test]
fn generated_capture_step_uses_pinned_diff_contract() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-contract-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = run_ripr_init(&root)?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // The capture moved from workflow shell into `ripr reports ci-packet`
    // (#4696): the workflow runs the command, and the command's capture
    // pins the presentation flags.
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let step = workflow_step_block(&workflow, "Run RIPR")
        .ok_or("generated workflow has no 'Run RIPR' step")?;
    assert!(step.contains("ripr reports ci-packet --root ."), "{step}");
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(manifest_dir.join("src/cli/commands/ci_packet.rs"))?;
    let capture = source
        .split("fn capture_pull_request_diff(")
        .nth(1)
        .and_then(|rest| rest.split("\n    fn ").next())
        .ok_or("ci_packet.rs has no capture_pull_request_diff")?;
    for pin in [
        "\"--no-ext-diff\"",
        "\"--no-textconv\"",
        "\"--no-color\"",
        "\"--src-prefix=a/\"",
        "\"--dst-prefix=b/\"",
        "\"--unified=3\"",
        "\"--inter-hunk-context=0\"",
        "\"core.quotePath=true\"",
        "\"--binary\"",
        "\"rev-parse\"",
        "Sha256::digest",
    ] {
        assert!(
            capture.contains(pin),
            "capture must pin {pin}; got:\n{capture}"
        );
    }

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: docs/CI.md documents the same capture step the generator emits.
/// The doc copy and the template copy drifted before (unpinned `git diff
/// --binary` in both); the capture block must stay byte-identical so the
/// documented recipe cannot promise a different patch than `ripr init` ships.
#[test]
fn docs_ci_run_step_matches_generated_template() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-doc-sync-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = run_ripr_init(&root)?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let generated = workflow_step_block(&workflow, "Run RIPR")
        .ok_or("generated workflow has no 'Run RIPR' step")?;
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc = fs::read_to_string(manifest_dir.join("../../docs/CI.md"))?;
    let documented =
        workflow_step_block(&doc, "Run RIPR").ok_or("docs/CI.md has no 'Run RIPR' step")?;
    assert_eq!(
        generated, documented,
        "docs/CI.md Run RIPR step drifted from the generated template"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: the pinned capture flags must retain a source edit that a
/// configured textconv driver hides. Control first: the pre-repair recipe
/// (`diff --binary` with ambient presentation) yields an empty patch, which
/// proves the fixture hides the edit. Then the repaired flag set — the same
/// pins the generated template now carries — must retain the edit with its
/// three-line context and no color bytes.
#[test]
fn pinned_capture_flags_retain_edit_hidden_by_textconv() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-textconv-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src"))?;
    fixture_git_ok(&root, &["init", "--initial-branch=main"])?;
    fixture_git_ok(
        &root,
        &["config", "--local", "user.name", "Capture Contract"],
    )?;
    fixture_git_ok(
        &root,
        &["config", "--local", "user.email", "capture@example.com"],
    )?;
    fixture_git_ok(&root, &["config", "--local", "commit.gpgsign", "false"])?;
    fs::write(root.join(".gitattributes"), "src/lib.rs diff=audit\n")?;
    fs::write(
        root.join("src/lib.rs"),
        "pub const A: u32 = 1;\npub const B: u32 = 2;\npub const C: u32 = 3;\npub const VALUE: u32 = 1;\npub const D: u32 = 4;\npub const E: u32 = 5;\npub const F: u32 = 6;\n",
    )?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "--quiet", "-m", "base"])?;
    fixture_git_ok(&root, &["tag", "capture-base"])?;
    fs::write(
        root.join("src/lib.rs"),
        "pub const A: u32 = 1;\npub const B: u32 = 2;\npub const C: u32 = 3;\npub const VALUE: u32 = 2;\npub const D: u32 = 4;\npub const E: u32 = 5;\npub const F: u32 = 6;\n",
    )?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])?;
    fixture_git_ok(&root, &["commit", "--quiet", "-m", "edit"])?;
    // Git itself is the constant-output helper on Unix and Windows; no
    // shell script or executable bit needed.
    fixture_git_ok(
        &root,
        &["config", "--local", "diff.audit.textconv", "git --version"],
    )?;
    let range = "capture-base...HEAD";
    let hidden = fixture_git_output(&root, &["diff", "--binary", range])?;
    assert!(
        hidden.trim().is_empty(),
        "the constant textconv must hide the source edit"
    );
    // Repaired recipe: the presentation pins the generated "Capture pull
    // request diff" step now carries (init.rs template + docs/CI.md).
    let retained = fixture_git_output(
        &root,
        &[
            "-c",
            "core.quotePath=true",
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--unified=3",
            "--inter-hunk-context=0",
            range,
        ],
    )?;
    assert!(
        retained.contains("pub const VALUE"),
        "pinned capture must retain the source edit despite textconv"
    );
    assert!(
        retained.contains("pub const C: u32 = 3;") && retained.contains("pub const D: u32 = 4;"),
        "pinned capture must keep context around the edit"
    );
    assert!(
        !retained.contains('\u{1b}'),
        "pinned capture must not contain color"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

/// #4005: execute the generated capture step itself in a disposable
/// repository. Unix-only: the generated workflow declares `runs-on:
/// ubuntu-latest`, so `sh` plus coreutils/`jq` are the faithful runner.
/// The `${{ github.base_ref }}` expression is substituted with a real local
/// branch (there is no origin in a fixture); everything else runs verbatim,
/// so this fails when the template stops being executable shell.
#[cfg(unix)]
#[test]
fn generated_capture_step_runs_end_to_end() -> Result<(), Box<dyn Error>> {
    // Runner tools: the generated workflow targets ubuntu-latest, where
    // bash, coreutils, and jq exist. On minimal local Unix environments
    // without them, skip loudly instead of failing the suite.
    let tools = run_sh(
        "command -v bash >/dev/null && command -v sha256sum >/dev/null && command -v jq >/dev/null",
        std::env::temp_dir().as_path(),
    )?;
    if !tools.status.success() {
        eprintln!(
            "skipping generated_capture_step_runs_end_to_end: bash, sha256sum, or jq not available"
        );
        return Ok(());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let init_root = std::env::temp_dir().join(format!(
        "ripr-generated-capture-exec-init-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&init_root)?;
    let output = run_ripr_init(&init_root)?;
    assert!(
        output.status.success(),
        "ripr init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let workflow = fs::read_to_string(init_root.join(".github/workflows/ripr.yml"))?;
    assert!(
        workflow.contains("        run: ripr reports ci-packet --root .\n"),
        "the generated workflow must run the packet command"
    );
    // Run the capture step of the command the workflow runs, as a pull
    // request into `base` would.
    let capture = |repo: &std::path::Path, base: &str| {
        replay::bash(
            repo,
            "ripr reports ci-packet --root . --step 'Capture pull request diff'",
            &[
                ("GITHUB_EVENT_NAME".to_string(), "pull_request".to_string()),
                ("GITHUB_BASE_REF".to_string(), base.to_string()),
                ("GITHUB_EVENT_PATH".to_string(), String::new()),
            ],
        )
    };

    let repo = std::env::temp_dir().join(format!(
        "ripr-generated-capture-exec-repo-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&repo)?;
    fixture_git_ok(&repo, &["init", "--initial-branch=main"])?;
    fixture_git_ok(&repo, &["config", "--local", "user.name", "Capture Exec"])?;
    fixture_git_ok(
        &repo,
        &[
            "config",
            "--local",
            "user.email",
            "capture-exec@example.com",
        ],
    )?;
    fixture_git_ok(&repo, &["config", "--local", "commit.gpgsign", "false"])?;
    fs::write(repo.join("probe.txt"), "before\n")?;
    fixture_git_ok(&repo, &["add", "."])?;
    fixture_git_ok(&repo, &["commit", "--quiet", "-m", "base"])?;
    fixture_git_ok(&repo, &["checkout", "--quiet", "-b", "feature"])?;
    fs::write(repo.join("probe.txt"), "after\n")?;
    fixture_git_ok(&repo, &["add", "probe.txt"])?;
    fixture_git_ok(&repo, &["commit", "--quiet", "-m", "edit"])?;
    let base_sha = fixture_git_output(&repo, &["rev-parse", "--verify", "main^{commit}"])?;
    let head_sha = fixture_git_output(&repo, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    // The workflow diffs against `origin/<base>`; a fixture has no remote.
    fixture_git_ok(&repo, &["update-ref", "refs/remotes/origin/main", "main"])?;
    fixture_git_ok(
        &repo,
        &["update-ref", "refs/remotes/origin/feature", "HEAD"],
    )?;

    // Make the ambient repository hostile to the parser's expected side
    // prefixes. The unpinned control proves the fixture actually changes Git's
    // presentation; the generated command must override it back to a/ and b/.
    fixture_git_ok(&repo, &["config", "--local", "diff.noprefix", "true"])?;
    let unpinned = fixture_git_output(&repo, &["diff", "main...HEAD"])?;
    assert!(
        unpinned.contains("diff --git probe.txt probe.txt"),
        "diff.noprefix control must remove side prefixes; got:\n{unpinned}"
    );

    // Positive: the step resolves main, captures the edit, and retains a
    // receipt whose identities and byte count match the run.
    let run = capture(&repo, "main")?;
    assert!(
        run.status.success(),
        "capture step failed: {}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let patch = fs::read(repo.join("target/ripr/reports/pr.diff"))?;
    let patch_text = String::from_utf8_lossy(&patch);
    assert!(
        patch_text.contains("after"),
        "captured patch must contain the feature-branch edit"
    );
    assert!(
        patch_text.contains("diff --git a/probe.txt b/probe.txt"),
        "generated capture must restore canonical side prefixes despite diff.noprefix; got:\n{patch_text}"
    );
    let receipt: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        repo.join("target/ripr/reports/pr-diff.receipt.json"),
    )?)?;
    let field = |name: &str| {
        receipt
            .get(name)
            .ok_or(format!("receipt is missing field {name:?}"))
    };
    let as_str = |name: &str| {
        field(name)?
            .as_str()
            .ok_or(format!("receipt field {name:?} is not a string"))
    };
    assert_eq!(
        as_str("tool")?,
        "ripr",
        "receipt must identify its producer"
    );
    assert_eq!(
        as_str("kind")?,
        "pr-diff-receipt",
        "receipt must identify its kind"
    );
    assert_eq!(
        as_str("base_ref")?,
        "origin/main",
        "receipt must retain the requested base ref"
    );
    assert_eq!(
        as_str("base_sha")?,
        base_sha.trim(),
        "receipt base_sha must equal the resolved base commit"
    );
    assert_eq!(
        as_str("head_sha")?,
        head_sha.trim(),
        "receipt head_sha must equal the resolved head commit"
    );
    assert_eq!(
        field("byte_count")?
            .as_u64()
            .ok_or("receipt field \"byte_count\" is not a number")?,
        patch.len() as u64,
        "receipt byte count must match the patch"
    );
    let expected_digest = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&patch)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        as_str("sha256")?,
        expected_digest.as_str(),
        "receipt digest must match the patch bytes"
    );

    // Negative: an unresolvable base fails closed with the named error
    // instead of handing RIPR an absent patch.
    let run = capture(&repo, "nonexistent-base-branch")?;
    assert!(
        !run.status.success(),
        "capture step must fail when the base cannot be resolved"
    );
    assert!(
        String::from_utf8_lossy(&run.stdout)
            .contains("::error title=Capture pull request diff::ripr: the pull request base `origin/nonexistent-base-branch` is not in this checkout"),
        "missing base must name the failure; got:\n{}",
        String::from_utf8_lossy(&run.stdout)
    );

    // Empty range: base == HEAD is an honest zero-change run — the step
    // succeeds with an empty patch and a zero byte count (exercises the
    // mktemp zero-change proof path with zero NUL bytes).
    let run = capture(&repo, "feature")?;
    assert!(
        run.status.success(),
        "capture step must accept a real zero-change range; got:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let empty_patch = fs::read(repo.join("target/ripr/reports/pr.diff"))?;
    assert!(
        empty_patch.is_empty(),
        "zero-change range must capture an empty patch"
    );
    let empty_receipt: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        repo.join("target/ripr/reports/pr-diff.receipt.json"),
    )?)?;
    assert_eq!(
        empty_receipt
            .get("byte_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "zero-change receipt must record a zero byte count"
    );

    fs::remove_dir_all(init_root)?;
    fs::remove_dir_all(repo)?;
    Ok(())
}

/// The existing-comment capture must read back the whole dedupe key the
/// publish step wrote. The key embeds the seam file path, so a path with a
/// space was cut at the space, no existing comment matched its
/// recommendation, and every rerun planned a duplicate create.
#[cfg(unix)]
#[test]
fn generated_existing_comment_capture_reads_keys_with_spaces() -> Result<(), Box<dyn Error>> {
    if !run_sh("command -v bash >/dev/null", std::env::temp_dir().as_path())?
        .status
        .success()
    {
        eprintln!(
            "skipping generated_existing_comment_capture_reads_keys_with_spaces: bash missing"
        );
        return Ok(());
    }

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-existing-comment-keys-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;
    let output = run_ripr_init(&root)?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&root);
        return Err(format!(
            "ripr init failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let script = existing_comment_capture_script(&workflow)?;

    // Bodies as the publish step writes them (compact marker) and as older
    // workflows wrote them (legacy marker), in `gh api --paginate --slurp`
    // page shape.
    let compact_key = "ripr:seam-1:src/we ird/pricing.rs:12";
    let legacy_key = "ripr:seam-2:src/lib.rs:3";
    let compact_body = format!(
        "**ripr: gap**\n\n<details><summary>Full RIPR repair card</summary>\n\ncard\n\n</details>\n\n<!-- ripr:dedupe={compact_key} presentation=compact-v1 -->"
    );
    let legacy_body = format!("legacy card\n\n<!-- ripr:dedupe={legacy_key} -->");
    // Negatives: a human comment with no marker, and a marker that never
    // closes. Neither may become an existing RIPR comment, and neither may
    // fail the step.
    let unmarked_body = "LGTM, but see ripr:dedupe docs";
    let unclosed_body = "<!-- ripr:dedupe=ripr:seam-3:src/x.rs:1";
    // Forged markers: a well-formed RIPR marker from a person, and one from
    // another bot. The workflow posts only as github-actions[bot], so
    // neither may suppress a card or become a PATCH target.
    let forged_user_body = "<!-- ripr:dedupe=ripr:seam-4:src/lib.rs:5 -->";
    let forged_bot_body = "<!-- ripr:dedupe=ripr:seam-5:src/lib.rs:6 -->";
    let actions_bot = r#"{"login":"github-actions[bot]","type":"Bot"}"#;
    let raw = format!(
        "[[{{\"id\":1,\"user\":{actions_bot},\"body\":{},\"path\":\"src/we ird/pricing.rs\",\"line\":12}},{{\"id\":2,\"user\":{actions_bot},\"body\":{},\"path\":\"src/lib.rs\",\"line\":3}},{{\"id\":3,\"user\":{actions_bot},\"body\":{},\"path\":\"src/lib.rs\",\"line\":4}},{{\"id\":4,\"user\":{actions_bot},\"body\":{},\"path\":\"src/x.rs\",\"line\":1}}],[{{\"id\":5,\"user\":{{\"login\":\"mallory\",\"type\":\"User\"}},\"body\":{},\"path\":\"src/lib.rs\",\"line\":5}},{{\"id\":6,\"user\":{{\"login\":\"other-app[bot]\",\"type\":\"Bot\"}},\"body\":{},\"path\":\"src/lib.rs\",\"line\":6}}]]",
        json_string(&compact_body),
        json_string(&legacy_body),
        json_string(unmarked_body),
        json_string(unclosed_body),
        json_string(forged_user_body),
        json_string(forged_bot_body)
    );
    // A stand-in `gh` prints the pages, so the step runs as written: its
    // output piped into the ripr under test.
    let bin = root.join("stand-in-bin");
    fs::create_dir_all(&bin)?;
    fs::write(root.join("pages.json"), &raw)?;
    fs::write(bin.join("gh"), "#!/bin/sh\ncat \"$PAGES\"\n")?;
    let ripr_dir = std::path::Path::new(env!("CARGO_BIN_EXE_ripr"))
        .parent()
        .ok_or("ripr binary has no directory")?;
    let script = format!(
        "chmod +x '{bin}/gh'\nexport PAGES='{pages}' PATH='{bin}:{ripr}':\"$PATH\" REPO=ripr-test/pricing PR_NUMBER=1\n{script}",
        bin = bin.display(),
        pages = root.join("pages.json").display(),
        ripr = ripr_dir.display(),
    );

    let run = run_sh(&script, &root)?;
    let captured = fs::read_to_string(root.join("target/ripr/review/existing-comments.json"));
    // The step keeps the raw pages beside the normalized file for the upload.
    let raw_copy = fs::read_to_string(root.join("target/ripr/review/existing-comments.raw.json"));
    let _ = fs::remove_dir_all(&root);
    assert_eq!(raw_copy.ok().as_deref(), Some(raw.as_str()));
    assert!(
        run.status.success(),
        "capture script failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let captured: serde_json::Value = serde_json::from_str(&captured?)?;
    let keys = captured["comments"]
        .as_array()
        .map(|comments| {
            comments
                .iter()
                .map(|comment| comment["dedupe_key"].as_str().unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(keys, vec![compact_key, legacy_key], "{captured}");
    assert_eq!(captured["comments"][0]["body"], "card", "{captured}");
    Ok(())
}

/// A pull request can commit files under `target/ripr` or `target/ci`
/// (`git add -f`) that later gate, ledger, and policy steps read when
/// present. The generated cleanup step must remove them, and only them,
/// before the first RIPR step.
#[cfg(unix)]
#[test]
fn generated_cleanup_step_removes_checked_in_ripr_artifacts() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-cleanup-forged-inputs-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;
    let output = run_ripr_init(&root)?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&root);
        return Err(format!(
            "ripr init failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let marker = "      - name: Remove checked-in RIPR artifacts\n        run: ";
    let at = workflow.find(marker).ok_or("missing cleanup step")?;
    let script = workflow[at + marker.len()..]
        .lines()
        .next()
        .ok_or("empty cleanup step")?
        .to_string();
    let first_ripr = workflow
        .find("      - name: Run RIPR\n")
        .ok_or("missing Run RIPR step")?;
    assert!(at < first_ripr, "cleanup must precede the first RIPR step");

    let forged = [
        "target/ripr/reports/sarif-policy.json",
        "target/ripr/reports/agent-receipt.json",
        "target/ripr/workflow/agent-verify.json",
        "target/ci/labels.json",
    ];
    for path in forged {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, "{}")?;
    }
    // Unrelated build output stays: the step is not a cache purge.
    fs::create_dir_all(root.join("target/debug"))?;
    fs::write(root.join("target/debug/keep"), "")?;

    let run = run_sh(&script, &root)?;
    let remaining = forged
        .iter()
        .filter(|path| root.join(path).exists())
        .collect::<Vec<_>>();
    let kept = root.join("target/debug/keep").exists();
    let _ = fs::remove_dir_all(&root);
    assert!(
        run.status.success(),
        "cleanup failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        remaining.is_empty(),
        "forged inputs remained: {remaining:?}"
    );
    assert!(kept, "cleanup removed unrelated target output");
    Ok(())
}

/// The capture step's `run:` body. GitHub expressions live in step `env:`
/// (`REPO`, `PR_NUMBER`); the caller exports those names.
#[cfg(unix)]
fn existing_comment_capture_script(workflow: &str) -> Result<String, String> {
    let block = workflow_step_block(workflow, "Capture existing RIPR inline comments")
        .ok_or("missing existing-comment step")?;
    let run_marker = "\n        run: |\n";
    let body = block
        .split_once(run_marker)
        .map(|(_, body)| body)
        .ok_or("missing existing-comment run")?;
    if !body.contains("ripr pr-comments existing --root . --raw -") {
        return Err(format!(
            "the capture step no longer pipes into ripr:\n{body}"
        ));
    }
    if body.contains("${{") {
        return Err(format!(
            "capture run: still interpolates a GitHub expression:\n{body}"
        ));
    }
    if !body.contains("repos/$REPO/pulls/$PR_NUMBER/comments") {
        return Err(format!(
            "capture run: no longer uses step env REPO/PR_NUMBER:\n{body}"
        ));
    }
    Ok(body.to_string())
}

/// #4089: the generated annotation step must keep path and message bytes.
/// The old `@tsv` | `read` loop stored jq's transport escapes, so a later
/// GitHub workflow-command decode could not recover a backslash, tab, or
/// line break.
#[cfg(unix)]
#[test]
fn generated_annotation_script_preserves_path_and_message_bytes() -> Result<(), Box<dyn Error>> {
    let tools = run_sh(
        "command -v bash >/dev/null && command -v jq >/dev/null",
        std::env::temp_dir().as_path(),
    )?;
    if !tools.status.success() {
        eprintln!(
            "skipping generated_annotation_script_preserves_path_and_message_bytes: bash or jq missing"
        );
        return Ok(());
    }

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-annotation-bytes-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;
    let output = run_ripr_init(&root)?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&root);
        return Err(format!(
            "ripr init failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    // Precondition: the generated workflow runs the packet command, whose
    // annotation step this test drives.
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    assert!(workflow.contains("        run: ripr reports ci-packet --root .\n"));

    let path_backslash = "src\\app.py";
    let path_newline = "src/a\nb.py";
    let reason_mixed = "line one\nline\ttwo\\nkept";
    let repair = "cargo test -- --exact";
    fs::create_dir_all(root.join("target/ripr/review"))?;
    fs::write(
        root.join("target/ripr/review/comments.json"),
        format!(
            r#"{{
  "comments": [
    {{
      "placement": {{"path": {path}, "line": 12}},
      "reason": {reason},
      "llm_guidance": {{"repair_command": {repair_json}}}
    }},
    {{
      "placement": {{"path": {backslash}, "line": 4}},
      "reason": "plain",
      "llm_guidance": {{"repair_command": ""}}
    }},
    {{
      "placement": {{"path": {newline}, "line": 7}},
      "reason": {mixed},
      "llm_guidance": {{"repair_command": "null"}}
    }},
    {{
      "placement": {{"path": "src/absent.py", "line": 1}},
      "reason": "no repair field",
      "llm_guidance": {{"repair_command": null}}
    }}
  ]
}}"#,
            path = json_string("src/app.py"),
            reason = json_string("Result::Err, 100%"),
            repair_json = json_string(repair),
            backslash = json_string(path_backslash),
            newline = json_string(path_newline),
            mixed = json_string(reason_mixed),
        ),
    )?;

    let ran = replay::bash(
        &root,
        "ripr reports ci-packet --root . --step 'Emit RIPR PR guidance annotations'",
        &[("GITHUB_EVENT_PATH".to_string(), String::new())],
    )?;
    let stdout = String::from_utf8_lossy(&ran.stdout).to_string();
    let stderr = String::from_utf8_lossy(&ran.stderr).to_string();
    if !ran.status.success() {
        let _ = fs::remove_dir_all(&root);
        return Err(format!("annotation step failed\nstderr: {stderr}\nstdout: {stdout}").into());
    }
    let emitted = stdout
        .lines()
        .filter(|line| line.starts_with("::warning "))
        .collect::<Vec<_>>();
    // Parity with the retired workflow jq program the step replaced: the
    // same input yields the same workflow commands, byte for byte.
    let retired = run_sh(
        &format!(
            "jq -r '{}' target/ripr/review/comments.json",
            RETIRED_ANNOTATION_JQ
        ),
        &root,
    )?;
    assert!(
        retired.status.success(),
        "{}",
        String::from_utf8_lossy(&retired.stderr)
    );
    assert_eq!(
        emitted,
        String::from_utf8_lossy(&retired.stdout)
            .lines()
            .collect::<Vec<_>>(),
        "the annotation step diverged from the retired jq program"
    );
    let mut warnings = Vec::new();
    for line in emitted {
        warnings.push(parse_warning(line.trim())?);
    }
    let _ = fs::remove_dir_all(&root);

    let ordinary = warnings
        .iter()
        .find(|row| row.0 == "src/app.py")
        .ok_or("missing ordinary annotation")?;
    assert_eq!(ordinary.1, "12");
    assert_eq!(ordinary.2, "RIPR targeted test guidance");
    assert_eq!(
        ordinary.3,
        format!("Result::Err, 100% Start the repair: {repair}")
    );

    let slashed = warnings
        .iter()
        .find(|row| row.0 == path_backslash)
        .ok_or("backslash path was rewritten")?;
    assert_eq!(slashed.1, "4");
    assert_eq!(slashed.3, "plain");

    let broken = warnings
        .iter()
        .find(|row| row.0 == path_newline)
        .ok_or("newline path was rewritten")?;
    assert_eq!(broken.1, "7");
    assert_eq!(broken.3, reason_mixed);

    let absent = warnings
        .iter()
        .find(|row| row.0 == "src/absent.py")
        .ok_or("null repair command dropped the row")?;
    assert_eq!(absent.3, "no repair field");
    assert!(!absent.3.contains("Start the repair"));
    Ok(())
}

/// #4468: the publish requests' compact body (once the step's jq, now
/// `ripr pr-comments requests`, #5409) must lift the
/// Start/Verify code span whatever its fence length. A command holding a
/// backtick is rendered with a longer fence (`output::markdown::code_span`);
/// the old single-backtick pattern missed it and fell back to a default
/// verify line. A body skipped as `comment_body_too_large` must count as an
/// additional recommendation in the review summary.
#[test]
fn generated_compact_body_lifts_code_spans_of_any_fence_length() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-compact-body-fence-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;
    let verify_body = "### ripr gap: missing boundary assertion\n\nChanged behavior:\n``a` b``\n\nWhy this matters:\nw\n\nRepair:\nAdd one.\n\nVerify:\n`` ripr agent verify `x` ``";
    let start_body = "### ripr gap: weak oracle\n\nWhy this matters:\nw\n\nRepair:\nAdd one.\n\nStart the repair:\n```ripr agent repair ``x`` --phase before```\n\nIt prints the `--attempt ... --phase after` command that verifies the new test.";
    let plain_body = "### ripr gap: weak oracle\n\nWhy this matters:\nw\n\nRepair:\nAdd one.\n\nVerify:\n`ripr agent verify`";
    let operation = |key: &str, body: &str| {
        serde_json::json!({"operation": "create", "safe_to_publish": true, "dedupe_key": key,
            "placement": {"path": "src/lib.rs", "line": 1}, "body": body})
    };
    let plan = serde_json::json!({
        "summary": {"safe_to_publish": true, "publishable": 3, "summary_only": 0, "suppressed": 0},
        "operations": [
            operation("ripr:verify", verify_body),
            operation("ripr:start", start_body),
            operation("ripr:plain", plain_body)
        ],
        "skipped": [{"skip_reason": "comment_body_too_large"}, {"skip_reason": "inline_comment_cap_reached"}]
    });
    let ran = run_pr_comments_requests(&root, &plan);
    let _ = fs::remove_dir_all(&root);
    let (requests, _) = ran?;
    let [review] = &requests[..] else {
        return Err(format!("expected one review request, got {requests:?}").into());
    };
    let bodies: Vec<&str> = review.payload["comments"]
        .as_array()
        .map(|comments| {
            comments
                .iter()
                .map(|comment| comment["body"].as_str().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    // The expectations are the retired jq program's outputs, unchanged.
    let starts = [
        "**ripr: missing boundary assertion** — Add one.\n\nVerify: `` ripr agent verify `x` ``\n\n<details>",
        "**ripr: weak oracle** — Add one.\n\nStart the repair: ```ripr agent repair ``x`` --phase before```\n\n<details>",
        // Ordinary single-backtick spans keep their previous compact form.
        "**ripr: weak oracle** — Add one.\n\nVerify: `ripr agent verify`\n\n<details>",
    ];
    assert_eq!(bodies.len(), starts.len(), "{bodies:?}");
    for (body, start) in bodies.iter().zip(starts) {
        assert!(body.starts_with(start), "{body}");
    }
    let summary = review.payload["body"].as_str().unwrap_or_default();
    assert!(
        summary.contains("2 additional recommendations remain"),
        "{summary}"
    );
    Ok(())
}

/// The CI summary's PR review summary and Recommended next test blocks,
/// collapsed full reports included, print commands a reader copies on
/// another machine. When `ripr agent start` bound them to the runner's
/// absolute checkout (#3999), the summary names the repository root `.`
/// instead, like the Agent review packet block; a sibling path that only
/// shares the checkout's prefix is left alone.
#[cfg(unix)]
#[test]
fn generated_summary_prints_repository_relative_commands() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir()
        .join(format!(
            "ripr-summary-relative-{}-{nonce}",
            std::process::id()
        ))
        .join("my repo");
    fs::create_dir_all(root.join("target/ripr/reports"))?;
    let root = root.canonicalize()?;

    let checkout = root.to_str().ok_or("non-utf8 temp path")?;
    let sibling = format!("{checkout}-other/notes.md");
    let verify = format!(
        "ripr agent verify --root '{checkout}' --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json > '{checkout}/target/ripr/workflow/agent-verify.json'"
    );
    let receipt = format!(
        "ripr agent receipt --root '{checkout}' --verify-json target/ripr/workflow/agent-verify.json --seam-id s1 --json"
    );
    let agent = format!("ripr agent brief --root '{checkout}' --seam-id s1 --json");
    fs::write(
        root.join("target/ripr/reports/pr-review-front-panel.json"),
        serde_json::json!({
            "status": "ready",
            "top_issue": {"verify_command": verify, "agent_command": agent},
        })
        .to_string(),
    )?;
    fs::write(
        root.join("target/ripr/reports/pr-review-front-panel.md"),
        format!("- Verify after the test edit: `{verify}`\n- Notes: `{sibling}`\n"),
    )?;
    fs::write(
        root.join("target/ripr/reports/first-useful-action.json"),
        serde_json::json!({
            "status": "ready",
            "commands": {"verify": verify, "receipt": receipt},
        })
        .to_string(),
    )?;
    fs::write(
        root.join("target/ripr/reports/first-useful-action.md"),
        format!("- Verify after the test edit: `{verify}`\n- Receipt after verify: `{receipt}`\n"),
    )?;

    // The generated step runs this command from the checkout root.
    let run = replay::ripr(&root, &["reports", "ci-summary", "--root", "."])?;
    assert!(
        run.status.success(),
        "summary command failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let summary = String::from_utf8(run.stdout)?;
    let relative_verify = "ripr agent verify --root '.' --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json > './target/ripr/workflow/agent-verify.json'";
    for heading in ["### PR review summary\n", "### Recommended next test\n"] {
        let block = summary
            .split(heading)
            .nth(1)
            .and_then(|rest| rest.split("\n### ").next())
            .ok_or_else(|| format!("summary has no {heading:?} block:\n{summary}"))?;
        let without_sibling = block.replace(&sibling, "");
        assert!(
            !without_sibling.contains(checkout),
            "{heading:?} prints the runner checkout path:\n{block}"
        );
        // The at-a-glance line and the collapsed full report both rewrite.
        assert_eq!(
            block.matches(relative_verify).count(),
            2,
            "{heading:?} must print verify at the repository root twice:\n{block}"
        );
    }
    assert!(
        summary.contains("ripr agent receipt --root '.' --verify-json"),
        "{summary}"
    );
    assert!(
        summary.contains("ripr agent brief --root '.' --seam-id s1 --json"),
        "{summary}"
    );
    assert!(
        summary.contains(&format!("- Notes: `{sibling}`")),
        "a path that only shares the checkout prefix must stay as written:\n{summary}"
    );

    if let Some(parent) = root.parent() {
        fs::remove_dir_all(parent)?;
    }
    Ok(())
}

/// Unix-only like its callers: the shell-backed tests that use this
/// helper are `#[cfg(unix)]`, and an ungated helper is dead code (and a
/// `-D warnings` failure) on Windows builds.
#[cfg(unix)]
/// The annotation step's jq program before `ripr reports ci-packet`
/// replaced it (#4696), kept as the parity oracle for the Rust renderer.
const RETIRED_ANNOTATION_JQ: &str = r#"
  def escape_data:
    gsub("%"; "%25") | gsub("\r"; "%0D") | gsub("\n"; "%0A");
  def escape_property:
    escape_data | gsub(":"; "%3A") | gsub(","; "%2C");
  .comments[]?
  | select(.placement.path and .placement.line)
  | (.llm_guidance.repair_command // "") as $repair_start
  | ((.reason // "RIPR targeted test guidance")
      + (if $repair_start != "" and $repair_start != "null"
         then " Start the repair: " + $repair_start
         else "" end)) as $message
  | "::warning file=\(.placement.path | escape_property),line=\(.placement.line | tostring | escape_property),title=RIPR targeted test guidance::\($message | escape_data)"
"#;

/// Unix-only like its callers: the shell-backed tests that use this
/// helper are `#[cfg(unix)]`, and an ungated helper is dead code (and a
/// `-D warnings` failure) on Windows builds.
#[cfg(unix)]
fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Unix-only like its callers: the shell-backed tests that use this
/// helper are `#[cfg(unix)]`, and an ungated helper is dead code (and a
/// `-D warnings` failure) on Windows builds.
#[cfg(unix)]
fn decode_github(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16)
                && matches!(byte, b'%' | b'\r' | b'\n' | b':' | b',')
            {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).map_err(|err| format!("annotation is not utf-8: {err}"))
}

/// Unix-only like its callers: the shell-backed tests that use this
/// helper are `#[cfg(unix)]`, and an ungated helper is dead code (and a
/// `-D warnings` failure) on Windows builds.
#[cfg(unix)]
fn parse_warning(line: &str) -> Result<(String, String, String, String), String> {
    let rest = line
        .strip_prefix("::warning ")
        .ok_or_else(|| format!("not a warning: {line}"))?;
    let (props, message) = rest
        .split_once("::")
        .ok_or_else(|| format!("no message separator: {line}"))?;
    let mut file = None;
    let mut line_no = None;
    let mut title = None;
    for part in props.split(',') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| format!("bad property: {part}"))?;
        let decoded = decode_github(value)?;
        match key {
            "file" => file = Some(decoded),
            "line" => line_no = Some(decoded),
            "title" => title = Some(decoded),
            other => return Err(format!("unexpected property {other}")),
        }
    }
    Ok((
        file.ok_or("missing file")?,
        line_no.ok_or("missing line")?,
        title.ok_or("missing title")?,
        decode_github(message)?,
    ))
}

/// Text between `open` and the next `close` in `haystack`.
fn between<'a>(haystack: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let rest = haystack.split_once(open)?.1;
    rest.split_once(close).map(|(inside, _)| inside)
}

/// #5874: the executable twin of the unix bash replay below. It needs no
/// shell, so it runs on Windows and Linux alike: it reads the generated
/// Install-step and summary-step shell and evaluates only what the failure
/// explanation depends on (the target `case`, the `why` and error templates,
/// the cargo fallback pin, and the summary `printf`) with the same variable
/// values the runner would supply. Corrupting the message in the template
/// fails here on every host.
#[test]
fn generated_install_failure_explanation_is_consistent_on_every_host() -> Result<(), Box<dyn Error>>
{
    let base = std::env::temp_dir().join(format!(
        "ripr-portable-failed-install-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir_all(&base)?;
    let init = run_ripr_init(&base)?;
    assert!(
        init.status.success(),
        "ripr init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let workflow = fs::read_to_string(base.join(".github/workflows/ripr.yml"))?;
    let install = workflow_step_block(&workflow, "Install ripr").ok_or("no `Install ripr` step")?;
    let summary_step = workflow_step_block(&workflow, "Add RIPR advisory summary")
        .ok_or("no `Add RIPR advisory summary` step")?;

    // Both install routes pin one version: the generated pin.
    let version = between(&install, "version=", "\n")
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.contains(' '))
        .ok_or("install step names no version pin")?
        .to_string();
    let cargo_pin = between(&install, "cargo install ripr --version ", " ")
        .ok_or("install step names no cargo fallback pin")?;
    assert_eq!(
        cargo_pin, version,
        "prebuilt and cargo routes must pin the same version"
    );

    // #6245: Windows-X64 now maps to the contract's MSVC zip; Windows-ARM64 is
    // an explicit unsupported error. The no-prebuilt route is exercised with a
    // runner shape that is genuinely unmapped (Linux-ARM): no `case` arm other
    // than the catch-all (which sets an empty target) may match it.
    let arms = install
        .lines()
        .filter_map(|line| line.trim().split_once(") target="))
        .collect::<Vec<_>>();
    assert!(!arms.is_empty(), "no target arms in {install}");
    let arm_for = |label: &str| {
        arms.iter()
            .find(|(pattern, _)| *pattern == label)
            .map(|(_, rest)| rest.trim_end_matches(" ;;"))
    };
    assert_eq!(
        arm_for("Windows-X64"),
        Some("x86_64-pc-windows-msvc"),
        "Windows-X64 must map to the contract's MSVC target"
    );
    assert!(
        install.contains("Windows-ARM64) echo \"::error::Unsupported runner Windows-ARM64: ripr $version publishes no prebuilt Windows ARM64 build"),
        "Windows-ARM64 must be an explicit unsupported error: {install}"
    );
    assert_eq!(arm_for("Linux-ARM"), None, "Linux-ARM must stay unmapped");
    let target = arm_for("*").ok_or("no catch-all target arm")?;
    assert!(
        target.trim_start().starts_with("\"\""),
        "the catch-all must keep the empty Cargo-fallback target ({target})"
    );

    // The no-prebuilt reason, the no-cargo error and the cargo notice, with
    // the runner's variable values.
    let render = |template: &str, why: &str| {
        template
            .replace("$RUNNER_OS", "Linux")
            .replace("$RUNNER_ARCH", "ARM")
            .replace("$version", &version)
            .replace("$why", why)
    };
    let why_template =
        between(&install, "why=\"", "\"").ok_or("no `why` reason in install step")?;
    let why = render(why_template, "");
    assert_eq!(why, format!("no prebuilt ripr {version} for Linux-ARM"));
    let error_template = between(
        &install,
        "echo \"::error::Cannot install ripr:",
        "\"; exit 1",
    )
    .ok_or("no no-cargo error line in install step")?;
    assert_eq!(
        render(error_template, &why),
        format!(
            " no prebuilt ripr {version} for Linux-ARM, and this runner has no cargo to build it. Install Rust on the runner (https://rustup.rs) or add a Rust toolchain step before Install ripr."
        )
    );
    // The error path exits nonzero before any cargo invocation.
    let (before_error, _) = install
        .split_once("::error::Cannot install ripr:")
        .ok_or("no error line")?;
    let guard = before_error
        .rsplit_once("command -v cargo")
        .ok_or("no cargo guard before the error line")?
        .1;
    assert!(!guard.contains("cargo install"), "{install}");
    // The guard is the negation: the error path runs only when cargo is absent.
    assert!(
        before_error.contains("if ! command -v cargo >/dev/null 2>&1; then"),
        "{install}"
    );
    let notice_template = between(&install, "echo \"::notice::", "\"")
        .ok_or("no cargo-fallback notice in install step")?;
    assert_eq!(
        render(notice_template, &why),
        format!("no prebuilt ripr {version} for Linux-ARM; building it with cargo install")
    );
    assert!(
        install.contains(&format!("cargo install ripr --version {version} --locked")),
        "{install}"
    );

    // The summary step after the failed install.
    let format = between(&summary_step, "printf '", "' \"${RIPR_INSTALL_OUTCOME")
        .ok_or("no summary printf in the summary step")?;
    let rendered = format.replace("%s", "failure").replace("\\n", "\n");
    assert!(
        rendered.starts_with("## RIPR advisory summary\n"),
        "{rendered}"
    );
    assert!(
        rendered.contains("the pinned ripr is not installed (Install ripr step: failure), so this run produced no RIPR reports."),
        "{rendered}"
    );
    assert!(
        rendered.contains("Next: open the Install ripr step log."),
        "{rendered}"
    );
    assert!(
        summary_step.contains(
            "::error::the pinned ripr is not installed (Install ripr step: ${RIPR_INSTALL_OUTCOME:-not run})"
        ),
        "{summary_step}"
    );
    assert!(
        summary_step.contains("RIPR_INSTALL_OUTCOME: ${{ steps.install.outcome }}"),
        "{summary_step}"
    );
    // The explanation runs only when the install did not succeed, fails the
    // step, and does so before the normal summary command.
    let guard_at = summary_step
        .find("if [ \"${RIPR_INSTALL_OUTCOME:-}\" != success ]; then")
        .ok_or("no install-outcome guard in the summary step")?;
    let printf_at = summary_step
        .find("printf '## RIPR advisory summary")
        .ok_or("no summary printf")?;
    let exit_at = summary_step
        .find("see that step's log.\"; exit 1")
        .ok_or("summary error line does not exit 1")?;
    let ci_summary_at = summary_step
        .find("ripr reports ci-summary")
        .ok_or("no ci-summary command")?;
    assert!(
        guard_at < printf_at && printf_at < exit_at && exit_at < ci_summary_at,
        "{summary_step}"
    );
    Ok(())
}

/// One spawn site for the built-binary `ripr init` invocations below
/// (process-policy bound).
/// #5236 review: without a prebuilt binary the install falls back to
/// `cargo install`, which needs Rust on the runner. A runner without cargo
/// fails the install with the cause and the fix, and the summary step, which
/// runs after the failure, says ripr is not installed and where to look
/// instead of printing nothing.
#[cfg(unix)]
#[test]
fn generated_workflow_explains_a_failed_install() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;
    if !replay::tool_available("bash") {
        if std::env::var_os("GITHUB_ACTIONS").is_some() {
            return Err("`bash` is not on PATH under GitHub Actions".into());
        }
        eprintln!("SKIPPED generated_workflow_explains_a_failed_install: `bash` is not on PATH");
        return Ok(());
    }
    let base = replay::unique_temp_dir("failed-install")?;
    let root = base.join("repo");
    fs::create_dir_all(&root)?;
    let init = run_ripr_init(&root)?;
    assert!(
        init.status.success(),
        "ripr init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let steps = replay::parse_steps(&workflow);
    let script = |name: &str| {
        steps
            .iter()
            .find(|step| step.name == name)
            .and_then(|step| step.run.clone())
            .ok_or_else(|| format!("no `{name}` run step"))
    };
    let install = script("Install ripr")?;
    let summary_step = script("Add RIPR advisory summary")?;

    // A runner PATH with the coreutils the step needs, but no download tool, cargo,
    // or ripr.
    let bin = base.join("bin");
    fs::create_dir_all(&bin)?;
    // The runner shell itself must resolve on that PATH too.
    let tool = |name: &str| {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
            .ok_or(format!("no {name} on PATH"))
    };
    let mkdir = tool("mkdir")?;
    let bash = tool("bash")?;
    std::os::unix::fs::symlink(&mkdir, bin.join("mkdir"))?;
    std::os::unix::fs::symlink(&bash, bin.join("bash"))?;
    let summary = base.join("step-summary.md");
    fs::write(&summary, "")?;
    let runner = |os: &str, arch: &str, path: &std::path::Path| {
        vec![
            ("PATH".to_string(), path.display().to_string()),
            ("RUNNER_OS".to_string(), os.to_string()),
            ("RUNNER_ARCH".to_string(), arch.to_string()),
            ("RUNNER_TEMP".to_string(), base.display().to_string()),
            (
                "GITHUB_PATH".to_string(),
                base.join("github-path").display().to_string(),
            ),
            (
                "GITHUB_ENV".to_string(),
                base.join("github-env").display().to_string(),
            ),
            (
                "GITHUB_STEP_SUMMARY".to_string(),
                summary.display().to_string(),
            ),
            ("RIPR_INSTALL_OUTCOME".to_string(), "failure".to_string()),
        ]
    };
    // #5208: the install step pins the generator itself when released, the
    // latest release when the generator is unreleased. Expect the pin the
    // generated workflow actually carries (parsed from both install routes,
    // which must agree), not unconditionally the package version.
    let version = install
        .split("version=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .ok_or("install step names no version pin")?
        .to_string();
    let cargo_pin = install
        .split("cargo install ripr --version ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .ok_or("install step names no cargo fallback pin")?;
    assert_eq!(
        cargo_pin, version,
        "prebuilt and cargo routes must pin the same version"
    );

    // No prebuilt archive for this runner shape (#6245: Windows-X64 is mapped now) and no cargo: fail and say both.
    let unmapped = replay::bash(&root, &install, &runner("Linux", "ARM", &bin))?;
    let unmapped_out = String::from_utf8_lossy(&unmapped.stdout);
    assert_eq!(unmapped.status.code(), Some(1), "{unmapped_out}");
    assert!(
        unmapped_out.contains(&format!(
            "::error::Cannot install ripr: no prebuilt ripr {version} for Linux-ARM, and this runner has no cargo to build it. Install Rust on the runner (https://rustup.rs) or add a Rust toolchain step before Install ripr."
        )),
        "{unmapped_out}"
    );
    // A failed download names the URL instead of the platform.
    let linux = replay::bash(&root, &install, &runner("Linux", "X64", &bin))?;
    let linux_out = String::from_utf8_lossy(&linux.stdout);
    assert_eq!(linux.status.code(), Some(1), "{linux_out}");
    assert!(
        linux_out.contains(&format!(
            "::error::Cannot install ripr: downloading https://github.com/EffortlessMetrics/ripr/releases/download/v{version}/ripr-server-v{version}-x86_64-unknown-linux-gnu.tar.gz failed, and this runner has no cargo"
        )),
        "{linux_out}"
    );

    // Alternate: with cargo on the runner the same fallback builds the
    // pinned version.
    let with_cargo = base.join("bin-cargo");
    fs::create_dir_all(&with_cargo)?;
    std::os::unix::fs::symlink(&mkdir, with_cargo.join("mkdir"))?;
    std::os::unix::fs::symlink(&bash, with_cargo.join("bash"))?;
    let cargo_args = base.join("cargo-args");
    for (name, body) in [
        ("cargo", format!("echo \"$@\" > '{}'", cargo_args.display())),
        ("ripr", format!("echo ripr {version}")),
    ] {
        let path = with_cargo.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    let built = replay::bash(&root, &install, &runner("Linux", "ARM", &with_cargo))?;
    let built_out = String::from_utf8_lossy(&built.stdout);
    assert!(built.status.success(), "{built_out}");
    assert!(
        built_out.contains(&format!(
            "::notice::no prebuilt ripr {version} for Linux-ARM; building it with cargo install"
        )),
        "{built_out}"
    );
    assert_eq!(
        fs::read_to_string(&cargo_args)?.trim(),
        format!("install ripr --version {version} --locked")
    );

    // The summary step after the failed install.
    let after = replay::bash(&root, &summary_step, &runner("Linux", "X64", &bin))?;
    let after_out = String::from_utf8_lossy(&after.stdout);
    assert_eq!(after.status.code(), Some(1), "{after_out}");
    let rendered = fs::read_to_string(&summary)?;
    assert!(
        rendered.starts_with("## RIPR advisory summary\n"),
        "{rendered}"
    );
    assert!(
        rendered.contains("the pinned ripr is not installed (Install ripr step: failure), so this run produced no RIPR reports."),
        "{rendered}"
    );
    assert!(
        rendered.contains("Next: open the Install ripr step log."),
        "{rendered}"
    );
    assert!(
        after_out
            .contains("::error::the pinned ripr is not installed (Install ripr step: failure)"),
        "{after_out}"
    );

    // Alternate: an older ripr already on the runner's PATH does not stand
    // in for the failed pinned install.
    fs::write(&summary, "")?;
    let stale = replay::bash(&root, &summary_step, &runner("Linux", "X64", &with_cargo))?;
    let stale_out = String::from_utf8_lossy(&stale.stdout);
    assert_eq!(stale.status.code(), Some(1), "{stale_out}");
    let rendered = fs::read_to_string(&summary)?;
    assert!(
        rendered.contains("the pinned ripr is not installed (Install ripr step: failure)"),
        "{rendered}"
    );
    assert!(!rendered.contains(&format!("ripr {version}")), "{rendered}");

    fs::remove_dir_all(base)?;
    Ok(())
}

/// #6245: replay support for the prebuilt-archive routes of the generated
/// `Install ripr` step. Everything here is unix-only (`bash`, `unzip`, symlinks
/// and `sha256sum` are the replay's tools), so it is gated rather than dead
/// code on Windows builds.
#[cfg(unix)]
mod archive_replay {
    use std::error::Error;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};

    pub(super) type TestResult<T> = Result<T, Box<dyn Error>>;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// One zip entry: stored (uncompressed), with unix mode bits so a symlink
    /// entry (`0o120777`) can be built. A hand-rolled writer keeps the replay
    /// free of a `zip` tool and can name entries (`..`, absolute, duplicate)
    /// that archivers refuse to create.
    pub(super) struct Entry<'a> {
        pub(super) name: &'a str,
        pub(super) data: &'a [u8],
        pub(super) mode: u32,
    }

    pub(super) fn file<'a>(name: &'a str, data: &'a [u8]) -> Entry<'a> {
        Entry {
            name,
            data,
            mode: 0o100_755,
        }
    }

    fn u16le(out: &mut Vec<u8>, value: usize) -> TestResult<()> {
        out.extend_from_slice(&u16::try_from(value)?.to_le_bytes());
        Ok(())
    }

    fn u32le(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn zip_bytes(entries: &[Entry<'_>]) -> TestResult<Vec<u8>> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for entry in entries {
            let offset = u32::try_from(out.len())?;
            let crc = crc32(entry.data);
            let size = u32::try_from(entry.data.len())?;
            u32le(&mut out, 0x0403_4b50);
            u16le(&mut out, 20)?;
            out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
            u32le(&mut out, crc);
            u32le(&mut out, size);
            u32le(&mut out, size);
            u16le(&mut out, entry.name.len())?;
            u16le(&mut out, 0)?;
            out.extend_from_slice(entry.name.as_bytes());
            out.extend_from_slice(entry.data);

            u32le(&mut central, 0x0201_4b50);
            u16le(&mut central, 0x0314)?;
            u16le(&mut central, 20)?;
            central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
            u32le(&mut central, crc);
            u32le(&mut central, size);
            u32le(&mut central, size);
            u16le(&mut central, entry.name.len())?;
            central.extend_from_slice(&[0; 8]);
            u32le(&mut central, entry.mode << 16);
            u32le(&mut central, offset);
            central.extend_from_slice(entry.name.as_bytes());
        }
        let central_offset = u32::try_from(out.len())?;
        let central_size = u32::try_from(central.len())?;
        out.extend_from_slice(&central);
        u32le(&mut out, 0x0605_4b50);
        out.extend_from_slice(&[0; 4]);
        u16le(&mut out, entries.len())?;
        u16le(&mut out, entries.len())?;
        u32le(&mut out, central_size);
        u32le(&mut out, central_offset);
        u16le(&mut out, 0)?;
        Ok(out)
    }

    pub(super) fn sha256_hex(path: &Path) -> TestResult<String> {
        let output = Command::new("sha256sum").arg(path).output()?;
        let text = String::from_utf8(output.stdout)?;
        Ok(text
            .split_whitespace()
            .next()
            .ok_or("sha256sum printed nothing")?
            .to_string())
    }

    fn script(path: &Path, body: &str) -> TestResult<()> {
        fs::write(path, format!("#!/bin/sh\n{body}\n"))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    /// One install-step run on a fresh fake runner: a `curl` that serves only
    /// the files placed in `served`, a `cargo` that records being called, an
    /// ambient `ripr`/`ripr.exe` on PATH that announces itself if it is ever
    /// run, and (for zip) a `System32/tar.exe` that stands in for bsdtar.
    pub(super) struct Case {
        pub(super) dir: PathBuf,
        pub(super) served: PathBuf,
        pub(super) temp: PathBuf,
        pub(super) curl_log: PathBuf,
        pub(super) cargo_log: PathBuf,
        pub(super) github_path: PathBuf,
        shims: PathBuf,
        ambient: PathBuf,
        systemroot: PathBuf,
    }

    impl Case {
        pub(super) fn new(label: &str, version: &str) -> TestResult<Case> {
            let dir = super::replay::unique_temp_dir(label)?;
            let served = dir.join("served");
            // Spaces and non-ASCII in the job-local directory (falsifier 6).
            let temp = dir.join("runner temp \u{fc}n\u{ef}c\u{f6}d\u{e9}");
            let shims = dir.join("shims");
            let ambient = dir.join("ambient");
            let systemroot = dir.join("winroot");
            for path in [
                &served,
                &temp,
                &shims,
                &ambient,
                &systemroot.join("System32"),
            ] {
                fs::create_dir_all(path)?;
            }
            let case = Case {
                curl_log: dir.join("curl.log"),
                cargo_log: dir.join("cargo.log"),
                github_path: dir.join("github-path"),
                dir,
                served,
                temp,
                shims,
                ambient,
                systemroot,
            };
            script(
                &case.shims.join("curl"),
                &format!(
                    "for a in \"$@\"; do case \"$a\" in http*) url=\"$a\";; esac; done\n\
                     echo \"$url\" >> '{}'\n\
                     f='{}'/\"$(basename \"$url\")\"\n\
                     [ -f \"$f\" ] || exit 22\n\
                     cp \"$f\" \"$(basename \"$url\")\"",
                    case.curl_log.display(),
                    case.served.display()
                ),
            )?;
            script(
                &case.shims.join("cargo"),
                &format!("echo \"$@\" >> '{}'", case.cargo_log.display()),
            )?;
            for name in ["ripr", "ripr.exe"] {
                script(
                    &case.ambient.join(name),
                    &format!("echo 'ripr {version} AMBIENT-RIPR-WAS-RUN'"),
                )?;
            }
            // Test double for Windows' bsdtar: lists and extracts through
            // `unzip`. It does not reproduce bsdtar's own path sanitizing.
            script(
                &case.systemroot.join("System32/tar.exe"),
                "case \"$1\" in\n\
                 -tf) exec unzip -Z1 \"$2\" ;;\n\
                 -xf) exec unzip -q \"$2\" -d \"$4\" ;;\n\
                 *) echo \"unexpected tar.exe args: $*\" >&2; exit 64 ;;\n\
                 esac",
            )?;
            fs::write(case.github_path.as_path(), "")?;
            fs::write(case.dir.join("github-env"), "")?;
            Ok(case)
        }

        pub(super) fn serve(&self, name: &str, bytes: &[u8]) -> TestResult<PathBuf> {
            let path = self.served.join(name);
            fs::write(&path, bytes)?;
            Ok(path)
        }

        /// Serve `asset` and its `.sha256` file naming the asset's real digest.
        pub(super) fn serve_verified(&self, asset: &str, bytes: &[u8]) -> TestResult<String> {
            let path = self.serve(asset, bytes)?;
            let digest = sha256_hex(&path)?;
            self.serve(&format!("{asset}.sha256"), format!("{digest}\n").as_bytes())?;
            Ok(digest)
        }

        pub(super) fn run(&self, install: &str, os: &str, arch: &str) -> TestResult<Output> {
            let inherited = std::env::var("PATH").unwrap_or_default();
            let path = format!(
                "{}:{}:{inherited}",
                self.shims.display(),
                self.ambient.display()
            );
            let env = [
                ("PATH", path),
                ("RUNNER_OS", os.to_string()),
                ("RUNNER_ARCH", arch.to_string()),
                ("RUNNER_TEMP", self.temp.display().to_string()),
                ("GITHUB_PATH", self.github_path.display().to_string()),
                (
                    "GITHUB_ENV",
                    self.dir.join("github-env").display().to_string(),
                ),
                ("SYSTEMROOT", self.systemroot.display().to_string()),
            ];
            let mut command = Command::new("bash");
            command
                .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", install])
                .current_dir(&self.dir)
                .env_clear()
                .stdin(Stdio::null());
            for (key, value) in env {
                command.env(key, value);
            }
            Ok(command.output()?)
        }

        pub(super) fn cargo_calls(&self) -> String {
            fs::read_to_string(&self.cargo_log).unwrap_or_default()
        }

        pub(super) fn curl_calls(&self) -> String {
            fs::read_to_string(&self.curl_log).unwrap_or_default()
        }

        pub(super) fn extracted(&self) -> Vec<String> {
            fs::read_dir(self.temp.join("ripr-bin"))
                .map(|dir| {
                    dir.filter_map(|entry| entry.ok())
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    /// The `Install ripr` run script and the version it pins, from a freshly
    /// generated workflow.
    pub(super) fn generated_install(label: &str) -> TestResult<(String, String, PathBuf)> {
        let base = super::replay::unique_temp_dir(label)?;
        let init = super::run_ripr_init(&base)?;
        assert!(
            init.status.success(),
            "ripr init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        let workflow = fs::read_to_string(base.join(".github/workflows/ripr.yml"))?;
        let install = super::replay::parse_steps(&workflow)
            .into_iter()
            .find(|step| step.name == "Install ripr")
            .and_then(|step| step.run)
            .ok_or("no `Install ripr` run step")?;
        let version = install
            .split("version=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .ok_or("install step names no version pin")?
            .to_string();
        Ok((install, version, base))
    }

    pub(super) fn tools_present() -> bool {
        let path = std::env::var_os("PATH").unwrap_or_default();
        ["bash", "unzip", "sha256sum", "tar"]
            .iter()
            .all(|tool| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
    }
}

/// #6245 falsifiers 1-6 and 8 on the Windows x64 ZIP route, replayed through
/// the generated `Install ripr` script under the workflow's
/// `bash --noprofile --norc -eo pipefail`. Linux stands in for the runner:
/// `curl` serves fixtures, and `System32/tar.exe` is an `unzip`-backed double
/// for bsdtar (see the test's PR notes for what that does not prove).
#[cfg(unix)]
#[test]
fn generated_install_windows_zip_route_replays_securely() -> Result<(), Box<dyn Error>> {
    use archive_replay::{Case, Entry, file, zip_bytes};
    if !archive_replay::tools_present() {
        if std::env::var_os("GITHUB_ACTIONS").is_some() {
            return Err("bash, unzip or sha256sum missing under GitHub Actions".into());
        }
        eprintln!("SKIPPED generated_install_windows_zip_route_replays_securely: tools missing");
        return Ok(());
    }
    let (install, version, base) = archive_replay::generated_install("windows-zip")?;
    let asset = format!("ripr-server-v{version}-x86_64-pc-windows-msvc.zip");
    let release = format!("https://github.com/EffortlessMetrics/ripr/releases/download/v{version}");
    let good_exe = format!("#!/bin/sh\necho \"ripr {version} via $0\"");
    let payload = |exe: &[u8]| -> Result<Vec<u8>, Box<dyn Error>> {
        zip_bytes(&[
            file("LICENSE-APACHE", b"a"),
            file("LICENSE-MIT", b"m"),
            file("README-server.txt", b"r"),
            file("ripr.exe", exe),
        ])
    };

    // Production-shaped success: exact asset and checksum requested, digest
    // verified before extraction, absolute ripr.exe run from a job-local
    // directory whose name has spaces and non-ASCII (falsifier 6), the
    // ambient ripr never used, and Cargo never consulted.
    let ok = Case::new("zip-ok", &version)?;
    let digest = ok.serve_verified(&asset, &payload(good_exe.as_bytes())?)?;
    let run = ok.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        run.status.code(),
        Some(0),
        "{out}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let exe_path = ok.temp.join("ripr-bin/ripr.exe");
    assert!(
        out.contains(&format!("via {}", exe_path.display())),
        "{out}"
    );
    assert!(!out.contains("AMBIENT-RIPR-WAS-RUN"), "{out}");
    assert!(
        out.contains(&format!(
            "::notice title=ripr install::version {version}, target x86_64-pc-windows-msvc, archive {asset}, sha256 {digest}, executable {}",
            exe_path.display()
        )),
        "{out}"
    );
    assert_eq!(
        ok.curl_calls(),
        format!("{release}/{asset}\n{release}/{asset}.sha256\n")
    );
    assert!(
        ok.cargo_calls().is_empty(),
        "cargo ran on the archive route"
    );
    assert!(fs::read_to_string(&ok.github_path)?.contains("ripr-bin"));

    // Falsifier 1: a corrupt checksum fails before extraction, with no Cargo
    // or PATH fallback.
    let corrupt = Case::new("zip-corrupt", &version)?;
    corrupt.serve(&asset, &payload(good_exe.as_bytes())?)?;
    corrupt.serve(
        &format!("{asset}.sha256"),
        format!("{}\n", "0".repeat(64)).as_bytes(),
    )?;
    let run = corrupt.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(
        out.contains("does not match its published SHA-256"),
        "{out}"
    );
    assert!(corrupt.extracted().is_empty(), "extracted before verifying");
    assert!(
        corrupt.cargo_calls().is_empty() && !out.contains("AMBIENT"),
        "{out}"
    );
    // ... an empty checksum file is not a pass ...
    let empty = Case::new("zip-empty-sum", &version)?;
    empty.serve(&asset, &payload(good_exe.as_bytes())?)?;
    empty.serve(&format!("{asset}.sha256"), b"")?;
    let run = empty.run(&install, "Windows", "X64")?;
    assert_eq!(run.status.code(), Some(1));
    assert!(empty.extracted().is_empty() && empty.cargo_calls().is_empty());
    // ... and a missing checksum asset fails rather than installing unverified.
    let missing = Case::new("zip-missing-sum", &version)?;
    missing.serve(&asset, &payload(good_exe.as_bytes())?)?;
    let run = missing.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(out.contains("refusing to install it unverified"), "{out}");
    assert!(missing.extracted().is_empty() && missing.cargo_calls().is_empty());

    // Falsifier 2: wrong, missing or nested executable fails the zip route.
    let shapes: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "missing",
            zip_bytes(&[file("LICENSE-MIT", b"m")])?,
            "does not contain ripr.exe at its root",
        ),
        (
            "wrong-name",
            zip_bytes(&[file("ripr", good_exe.as_bytes())])?,
            "does not contain ripr.exe at its root",
        ),
        (
            "nested",
            zip_bytes(&[file("ripr-server/ripr.exe", good_exe.as_bytes())])?,
            "outside a flat archive root",
        ),
        (
            // Falsifier 5: traversal, absolute, drive-letter, backslash and
            // duplicate entries and a symlink entry are all refused.
            "traversal",
            zip_bytes(&[
                file("../evil.exe", b"x"),
                file("ripr.exe", good_exe.as_bytes()),
            ])?,
            "outside a flat archive root",
        ),
        (
            "absolute",
            zip_bytes(&[
                file("/tmp/ripr-evil", b"x"),
                file("ripr.exe", good_exe.as_bytes()),
            ])?,
            "outside a flat archive root",
        ),
        (
            "drive-letter",
            zip_bytes(&[
                file("C:evil.exe", b"x"),
                file("ripr.exe", good_exe.as_bytes()),
            ])?,
            "outside a flat archive root",
        ),
        (
            "backslash",
            zip_bytes(&[
                file("..\\evil.exe", b"x"),
                file("ripr.exe", good_exe.as_bytes()),
            ])?,
            "outside a flat archive root",
        ),
        (
            "duplicate",
            zip_bytes(&[
                file("ripr.exe", good_exe.as_bytes()),
                file("ripr.exe", b"#!/bin/sh\necho evil"),
            ])?,
            "duplicate entry",
        ),
        (
            "not-a-zip",
            b"this is not a zip archive".to_vec(),
            "is not a readable zip archive",
        ),
    ];
    for (label, bytes, message) in shapes {
        let case = Case::new(&format!("zip-{label}"), &version)?;
        case.serve_verified(&asset, &bytes)?;
        let run = case.run(&install, "Windows", "X64")?;
        let out = String::from_utf8_lossy(&run.stdout);
        assert_eq!(run.status.code(), Some(1), "{label}: {out}");
        assert!(out.contains(message), "{label}: {out}");
        assert!(
            case.extracted().is_empty(),
            "{label}: extracted {:?}",
            case.extracted()
        );
        assert!(case.cargo_calls().is_empty(), "{label}: fell back to cargo");
        assert!(!out.contains("AMBIENT"), "{label}: {out}");
        assert!(
            !case.dir.join("evil.exe").exists(),
            "{label}: escaped the archive root"
        );
    }
    // A very large flat listing whose first-sorted entry is a traversal name
    // must still be refused before extraction: a `printf | grep -q` pipeline
    // under `pipefail` could lose that match to SIGPIPE and read "clean".
    let names: Vec<String> = (0..60_000)
        .map(|n| format!("filler-entry-{n:06}.txt"))
        .collect();
    let mut entries: Vec<Entry<'_>> =
        vec![file("../x", b"x"), file("ripr.exe", good_exe.as_bytes())];
    entries.extend(names.iter().map(|name| file(name, b"")));
    let large = Case::new("zip-large-listing", &version)?;
    large.serve_verified(&asset, &zip_bytes(&entries)?)?;
    let run = large.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(out.contains("outside a flat archive root"), "{out}");
    assert!(large.extracted().is_empty() && large.cargo_calls().is_empty());
    // ... and a large clean listing still finds ripr.exe and installs.
    let clean = Case::new("zip-large-clean", &version)?;
    let mut entries: Vec<Entry<'_>> = vec![file("ripr.exe", good_exe.as_bytes())];
    entries.extend(names.iter().map(|name| file(name, b"")));
    clean.serve_verified(&asset, &zip_bytes(&entries)?)?;
    let run = clean.run(&install, "Windows", "X64")?;
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );

    // The reported version must equal the pin, not merely start with it.
    for reported in [
        format!("{version}0"),
        format!("{version}-rc.1"),
        format!("{version}.9"),
    ] {
        let case = Case::new("zip-version-prefix", &version)?;
        case.serve_verified(
            &asset,
            &payload(format!("#!/bin/sh\necho 'ripr {reported} (abc)'").as_bytes())?,
        )?;
        let run = case.run(&install, "Windows", "X64")?;
        let out = String::from_utf8_lossy(&run.stdout);
        assert_eq!(run.status.code(), Some(1), "{reported}: {out}");
        assert!(
            out.contains(&format!("not ripr {version}")),
            "{reported}: {out}"
        );
    }

    // A symlink entry named ripr.exe passes the listing but is not a regular
    // file once extracted.
    let link = Case::new("zip-symlink", &version)?;
    link.serve_verified(
        &asset,
        &zip_bytes(&[Entry {
            name: "ripr.exe",
            data: b"/bin/sh",
            mode: 0o120_777,
        }])?,
    )?;
    let run = link.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(
        out.contains("did not extract a regular file named ripr.exe"),
        "{out}"
    );
    assert!(link.cargo_calls().is_empty());

    // Falsifier 3: a checksum published for another target's archive does not
    // vouch for this one, and a payload that reports another version is
    // refused.
    let wrong_sum = Case::new("zip-wrong-target-sum", &version)?;
    wrong_sum.serve(&asset, &payload(good_exe.as_bytes())?)?;
    let other = wrong_sum.serve(
        &format!("ripr-server-v{version}-aarch64-unknown-linux-gnu.tar.gz"),
        b"another target's archive",
    )?;
    wrong_sum.serve(
        &format!("{asset}.sha256"),
        format!("{}\n", archive_replay::sha256_hex(&other)?).as_bytes(),
    )?;
    let run = wrong_sum.run(&install, "Windows", "X64")?;
    assert_eq!(run.status.code(), Some(1));
    assert!(wrong_sum.extracted().is_empty() && wrong_sum.cargo_calls().is_empty());
    let wrong_version = Case::new("zip-wrong-version", &version)?;
    wrong_version.serve_verified(&asset, &payload(b"#!/bin/sh\necho 'ripr 0.0.1 (other)'")?)?;
    let run = wrong_version.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(out.contains(&format!("not ripr {version}")), "{out}");
    assert!(wrong_version.cargo_calls().is_empty());

    // Falsifier 4: an ambient ripr/ripr.exe on PATH never stands in for a
    // broken extracted candidate.
    let broken = Case::new("zip-broken-exe", &version)?;
    broken.serve_verified(&asset, &payload(b"#!/bin/sh\nexit 3")?)?;
    let run = broken.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(3), "{out}");
    assert!(!out.contains("AMBIENT"), "{out}");
    assert!(broken.cargo_calls().is_empty());

    // Alternate: no published archive at all is the one case that still
    // reaches the documented Cargo fallback (the asset is simply unavailable).
    let absent = Case::new("zip-absent", &version)?;
    let run = absent.run(&install, "Windows", "X64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(
        out.contains(&format!(
            "::notice::downloading {release}/{asset} failed; building it with cargo install"
        )),
        "{out}"
    );
    assert_eq!(
        absent.cargo_calls().trim(),
        format!("install ripr --version {version} --locked")
    );

    // Falsifier 8: Windows ARM64 is an explicit unsupported route, not x64
    // success and not a Cargo build.
    let arm = Case::new("zip-arm64", &version)?;
    arm.serve_verified(&asset, &payload(good_exe.as_bytes())?)?;
    let run = arm.run(&install, "Windows", "ARM64")?;
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(run.status.code(), Some(1), "{out}");
    assert!(
        out.contains("::error::Unsupported runner Windows-ARM64:"),
        "{out}"
    );
    assert!(
        arm.curl_calls().is_empty() && arm.cargo_calls().is_empty() && arm.extracted().is_empty()
    );

    fs::remove_dir_all(base)?;
    Ok(())
}

/// #6245 falsifier 9 and the tar.gz side of falsifier 1/2: the Linux and macOS
/// routes still request exactly their contract archive, verify before
/// extraction, run the extracted binary by absolute path, and never fall back
/// to Cargo after an integrity or archive rejection.
#[cfg(unix)]
#[test]
fn generated_install_unix_routes_keep_their_target_mapping_and_fail_closed()
-> Result<(), Box<dyn Error>> {
    use archive_replay::Case;
    if !archive_replay::tools_present() {
        if std::env::var_os("GITHUB_ACTIONS").is_some() {
            return Err("bash, unzip or sha256sum missing under GitHub Actions".into());
        }
        eprintln!("SKIPPED generated_install_unix_routes_keep_their_target_mapping: tools missing");
        return Ok(());
    }
    let (install, version, base) = archive_replay::generated_install("unix-mapping")?;
    let release = format!("https://github.com/EffortlessMetrics/ripr/releases/download/v{version}");
    let routes = [
        ("Linux", "X64", "x86_64-unknown-linux-gnu"),
        ("Linux", "ARM64", "aarch64-unknown-linux-gnu"),
        ("macOS", "X64", "x86_64-apple-darwin"),
        ("macOS", "ARM64", "aarch64-apple-darwin"),
    ];
    for (os, arch, target) in routes {
        let asset = format!("ripr-server-v{version}-{target}.tar.gz");
        // Nothing published: the exact contract asset is requested, then the
        // documented Cargo fallback runs.
        let absent = Case::new("unix-absent", &version)?;
        let run = absent.run(&install, os, arch)?;
        let out = String::from_utf8_lossy(&run.stdout);
        assert_eq!(
            absent.curl_calls(),
            format!("{release}/{asset}\n"),
            "{os}-{arch}: {out}"
        );
        assert_eq!(
            absent.cargo_calls().trim(),
            format!("install ripr --version {version} --locked"),
            "{os}-{arch}: {out}"
        );

        // Published and verified: extracted and run by absolute path.
        let ok = Case::new("unix-ok", &version)?;
        let staging = ok.dir.join("staging");
        fs::create_dir_all(&staging)?;
        fs::write(
            staging.join("ripr"),
            format!("#!/bin/sh\necho \"ripr {version} via $0\"\n"),
        )?;
        let mut perms = fs::metadata(staging.join("ripr"))?.permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        fs::set_permissions(staging.join("ripr"), perms)?;
        let packed = ok.served.join(&asset);
        let tar = Command::new("tar")
            .args(["-czf"])
            .arg(&packed)
            .arg("-C")
            .arg(&staging)
            .arg("ripr")
            .status()?;
        assert!(tar.success());
        let digest = archive_replay::sha256_hex(&packed)?;
        ok.serve(&format!("{asset}.sha256"), format!("{digest}\n").as_bytes())?;
        let run = ok.run(&install, os, arch)?;
        let out = String::from_utf8_lossy(&run.stdout);
        assert_eq!(run.status.code(), Some(0), "{os}-{arch}: {out}");
        assert!(
            out.contains(&format!("via {}", ok.temp.join("ripr-bin/ripr").display())),
            "{os}-{arch}: {out}"
        );
        assert!(
            !out.contains("AMBIENT") && ok.cargo_calls().is_empty(),
            "{os}-{arch}: {out}"
        );
        assert_eq!(
            ok.curl_calls(),
            format!("{release}/{asset}\n{release}/{asset}.sha256\n")
        );

        // Corrupt checksum and a malformed archive both stop the step.
        let bad = Case::new("unix-corrupt", &version)?;
        bad.serve(&asset, &fs::read(&packed)?)?;
        bad.serve(
            &format!("{asset}.sha256"),
            format!("{}\n", "f".repeat(64)).as_bytes(),
        )?;
        let run = bad.run(&install, os, arch)?;
        assert_eq!(run.status.code(), Some(1), "{os}-{arch}");
        assert!(bad.extracted().is_empty() && bad.cargo_calls().is_empty());
        let garbage = Case::new("unix-garbage", &version)?;
        garbage.serve_verified(&asset, b"not a gzip stream")?;
        let run = garbage.run(&install, os, arch)?;
        let out = String::from_utf8_lossy(&run.stdout);
        assert_eq!(run.status.code(), Some(1), "{os}-{arch}: {out}");
        assert!(out.contains("could not be extracted"), "{os}-{arch}: {out}");
        assert!(garbage.cargo_calls().is_empty() && !out.contains("AMBIENT"));
    }
    fs::remove_dir_all(base)?;
    Ok(())
}

/// The publish step's loop sends every request ripr wrote, in order, even
/// when `gh` reads standard input: the loop's own stdin is the manifest, so
/// a `gh` that drained it would silently skip every later request.
#[cfg(unix)]
#[test]
fn generated_publish_step_sends_every_request_in_order() -> Result<(), Box<dyn Error>> {
    if !run_sh("command -v bash >/dev/null", std::env::temp_dir().as_path())?
        .status
        .success()
    {
        eprintln!("skipping generated_publish_step_sends_every_request_in_order: bash missing");
        return Ok(());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-publish-loop-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(String, String), Box<dyn Error>> {
        let init = run_ripr_init(&root)?;
        if !init.status.success() {
            return Err("ripr init failed".into());
        }
        let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
        let block = workflow_step_block(&workflow, "Publish RIPR inline comments")
            .ok_or("missing publish step")?;
        let body = block
            .split_once("\n        run: |\n")
            .map(|(_, body)| body)
            .ok_or("publish step has no run block")?;
        if body.contains("${{") {
            return Err("publish run: still interpolates a GitHub expression".into());
        }
        let plan = serde_json::json!({
            "summary": {"safe_to_publish": true, "publishable": 2},
            "operations": [
                {"operation": "update", "safe_to_publish": true, "dedupe_key": "ripr:b",
                 "existing_comment_id": 77, "body": "card b"},
                {"operation": "create", "safe_to_publish": true, "dedupe_key": "ripr:a",
                 "placement": {"path": "src/lib.rs", "line": 1}, "body": "card a"}
            ]
        });
        fs::create_dir_all(root.join("target/ripr/review"))?;
        fs::write(
            root.join("target/ripr/review/comment-publish-plan.json"),
            plan.to_string(),
        )?;
        // A stand-in `gh` that logs its arguments and drains stdin, as a
        // real one waiting on input would.
        let bin = root.join("stand-in-bin");
        fs::create_dir_all(&bin)?;
        let log = root.join("gh.log");
        fs::write(
            bin.join("gh"),
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}'\ncat >/dev/null\n",
                log.display()
            ),
        )?;
        let ripr_dir = std::path::Path::new(env!("CARGO_BIN_EXE_ripr"))
            .parent()
            .ok_or("ripr binary has no directory")?;
        let script = format!(
            "chmod +x '{bin}/gh'\nexport PATH='{bin}:{ripr}':\"$PATH\" REPO=ripr-test/pricing PR_NUMBER=42 PR_HEAD_SHA=0123abcd\n{body}",
            bin = bin.display(),
            ripr = ripr_dir.display(),
        );
        let run = run_sh(&script, &root)?;
        if !run.status.success() {
            return Err(format!(
                "publish step failed: {}",
                String::from_utf8_lossy(&run.stderr)
            )
            .into());
        }
        Ok((
            fs::read_to_string(&log)?,
            String::from_utf8_lossy(&run.stdout).to_string(),
        ))
    })();
    let _ = fs::remove_dir_all(&root);
    let (log, stdout) = result?;
    let calls: Vec<&str> = log.lines().collect();
    assert_eq!(
        calls,
        vec![
            "api --method PATCH repos/ripr-test/pricing/pulls/comments/77 --input target/ripr/review/publish/01-patch.json",
            "api --method POST repos/ripr-test/pricing/pulls/42/reviews --input target/ripr/review/publish/02-post.json",
        ]
    );
    assert_eq!(
        stdout,
        "Updated RIPR inline comment: ripr:b\nCreated one RIPR review with 1 inline comment(s).\n"
    );
    Ok(())
}

/// The summary step, replayed with a stand-in `gh`: the earlier bot comment
/// is found in the conversation pages and PATCHed, not duplicated.
/// Unix-only: the stand-in CLI uses a POSIX shell script.
#[cfg(unix)]
#[test]
fn summary_step_updates_the_bot_comment_it_posted_before() -> Result<(), Box<dyn Error>> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-summary-step-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(String, String), Box<dyn Error>> {
        let init = run_ripr_init(&root)?;
        if !init.status.success() {
            return Err("ripr init failed".into());
        }
        let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
        let block = workflow_step_block(&workflow, "Publish RIPR summary comment")
            .ok_or("missing summary step")?;
        let body = block
            .split_once("\n        run: |\n")
            .map(|(_, body)| body)
            .ok_or("summary step has no run block")?;
        if body.contains("${{") {
            return Err("summary run: still interpolates a GitHub expression".into());
        }
        fs::create_dir_all(root.join("target/ripr/review"))?;
        fs::write(
            root.join("target/ripr/review/comments.json"),
            serde_json::json!({
                "comments": [{
                    "seam": {"file": "src/lib.rs", "line": 2},
                    "owner": "src/lib.rs::discount",
                    "kind": "predicate_boundary",
                    "missing_discriminator": "5 (equality boundary)"
                }],
                "summary": {"summary_only": 0}
            })
            .to_string(),
        )?;
        let bin = root.join("stand-in-bin");
        fs::create_dir_all(&bin)?;
        let log = root.join("gh.log");
        // Pages for the listing call; every other call is logged.
        let pages = root.join("pages.json");
        fs::write(
            &pages,
            serde_json::json!([[{
                "id": 5,
                "body": "<!-- ripr:summary -->\nold",
                "user": {"login": "github-actions[bot]", "type": "Bot"}
            }]])
            .to_string(),
        )?;
        fs::write(
            bin.join("gh"),
            format!(
                "#!/bin/sh\ncase \"$*\" in\n*--paginate*) cat '{}' ;;\n*) echo \"$*\" >> '{}'; cat >/dev/null ;;\nesac\n",
                pages.display(),
                log.display()
            ),
        )?;
        let ripr_dir = std::path::Path::new(env!("CARGO_BIN_EXE_ripr"))
            .parent()
            .ok_or("ripr binary has no directory")?;
        let script = format!(
            "chmod +x '{bin}/gh'\nexport PATH='{bin}:{ripr}':\"$PATH\"\nexport GITHUB_SERVER_URL=https://github.com GITHUB_REPOSITORY=ripr-test/pricing GITHUB_RUN_ID=9 REPO=ripr-test/pricing PR_NUMBER=42 PR_HEAD_SHA=0123abcd\n{body}",
            bin = bin.display(),
            ripr = ripr_dir.display(),
        );
        let run = run_sh(&script, &root)?;
        if !run.status.success() {
            return Err(format!(
                "summary step failed: {}",
                String::from_utf8_lossy(&run.stderr)
            )
            .into());
        }
        let payload =
            fs::read_to_string(root.join("target/ripr/review/summary/summary-request.json"))?;
        Ok((fs::read_to_string(&log)?, payload))
    })();
    let _ = fs::remove_dir_all(&root);
    let (log, payload) = result?;
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        vec![
            "api --method PATCH repos/ripr-test/pricing/issues/comments/5 --input target/ripr/review/summary/summary-request.json"
        ]
    );
    assert!(payload.contains("actions/runs/9"), "{payload}");
    assert!(payload.contains("5 (equality boundary)"), "{payload}");
    Ok(())
}

/// One request `ripr pr-comments requests` wrote, as the publish step
/// replays it.
#[derive(Debug)]
struct PublishRequest {
    method: String,
    endpoint: String,
    payload: serde_json::Value,
    message: String,
}

/// Runs `ripr pr-comments requests` on `plan` for pull request 42 at head
/// `0123abcd`, and reads back its manifest and request files.
fn run_pr_comments_requests(
    root: &std::path::Path,
    plan: &serde_json::Value,
) -> Result<(Vec<PublishRequest>, String), Box<dyn Error>> {
    fs::create_dir_all(root.join("target/ripr/review"))?;
    fs::write(
        root.join("target/ripr/review/comment-publish-plan.json"),
        plan.to_string(),
    )?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args([
            "pr-comments",
            "requests",
            "--root",
            ".",
            "--pull-request",
            "42",
            "--head-sha",
            "0123abcd",
        ])
        .current_dir(root)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
        return Err(format!(
            "ripr pr-comments requests failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let manifest = fs::read_to_string(root.join("target/ripr/review/publish/requests.tsv"))?;
    let mut requests = Vec::new();
    for line in manifest.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [method, endpoint, file, message] = fields[..] else {
            return Err(format!("manifest line is not four tab-separated fields: {line:?}").into());
        };
        let payload = serde_json::from_str(&fs::read_to_string(root.join(file))?)?;
        requests.push(PublishRequest {
            method: method.to_string(),
            endpoint: endpoint.to_string(),
            payload,
            message: message.to_string(),
        });
    }
    Ok((requests, stdout))
}

/// One spawn site for running `ripr` with `args` from `cwd` (process-policy
/// bound).
fn run_ripr_in(
    cwd: &std::path::Path,
    args: &[&str],
) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(cwd)
        .args(args)
        .output()?)
}

fn run_ripr_init(root: &std::path::Path) -> Result<std::process::Output, Box<dyn Error>> {
    let root = root.to_string_lossy();
    run_ripr_in(
        std::path::Path::new("."),
        &["init", "--root", root.as_ref(), "--ci", "github"],
    )
}

/// One spawn site for executing extracted capture-step shell (process-policy
/// bound). The generated workflow pins `defaults.run.shell: bash`, which
/// GitHub Actions runs as `bash --noprofile --norc -eo pipefail {0}`, so the
/// helper mirrors that invocation instead of a bare `sh -c`, which would
/// enable neither errexit nor pipefail. Unix-only, so Windows builds never
/// see a dead helper.
#[cfg(unix)]
fn run_sh(script: &str, cwd: &std::path::Path) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new("bash")
        .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script])
        .current_dir(cwd)
        .output()?)
}

/// #4452: GitHub expressions in `run:` shell text are template injection.
/// `concurrency:`, `with: ref:`, cache `key:`, and `if:` may still use `${{`.
fn assert_no_github_expression_in_run_blocks(workflow: &str) {
    let mut in_run = false;
    let mut run_indent = 0usize;
    for line in workflow.lines() {
        if in_run {
            let indent = line.chars().take_while(|c| *c == ' ').count();
            if line.is_empty() || indent > run_indent {
                assert!(
                    !line.contains("${{"),
                    "generated run: block interpolates a GitHub expression:\n{line}"
                );
                continue;
            }
            in_run = false;
        }
        let indent = line.chars().take_while(|c| *c == ' ').count();
        let trimmed = line[indent..].trim_end();
        let Some(rest) = trimmed.strip_prefix("run:") else {
            continue;
        };
        let rest = rest.trim_start();
        if rest == "|" || rest == ">" {
            in_run = true;
            run_indent = indent;
            continue;
        }
        assert!(
            !rest.contains("${{"),
            "generated run: interpolates a GitHub expression:\n{line}"
        );
    }
}

/// Extract one step block: from its `- name:` line through (excluding) the
/// next step's `- name:` line or the end of a Markdown code fence.
fn workflow_step_block(text: &str, name: &str) -> Option<String> {
    let marker = format!("- name: {name}\n");
    let start = text.find(&marker)?;
    let rest = &text[start..];
    let end = ["\n      - name: ", "\n```"]
        .iter()
        .filter_map(|stop| rest[marker.len()..].find(stop))
        .min()
        .map(|offset| marker.len() + offset)
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end().to_string())
}

/// The generated summary step runs `ripr reports ci-summary --root .` with
/// no setting flags, so the base ref, SARIF upload and comment mode reach
/// the summary only through the step environment (#6723). Each setting is
/// checked against the same run without it, so a summary that stops
/// reading the environment fails here.
#[cfg(unix)]
#[test]
fn ci_summary_reads_the_workflow_settings_from_the_environment() -> Result<(), Box<dyn Error>> {
    let root = replay::unique_temp_dir("ci-summary-env")?;
    let summary = |settings: &[(&str, &str)]| -> Result<String, Box<dyn Error>> {
        let output = replay::ripr_with_env(
            &root,
            &["reports", "ci-summary", "--root", "."],
            &[
                "GITHUB_BASE_REF",
                "GITHUB_EVENT_PATH",
                "RIPR_UPLOAD_SARIF",
                "RIPR_COMMENT_MODE",
                "RIPR_GATE_BASELINE",
                "RIPR_GATE_MODE",
            ],
            settings,
        )?;
        if !output.status.success() {
            return Err(format!(
                "ci-summary failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?)
    };
    let unset = summary(&[]);
    let set = summary(&[
        ("GITHUB_BASE_REF", "release"),
        ("RIPR_UPLOAD_SARIF", "true"),
        ("RIPR_COMMENT_MODE", "inline"),
    ]);
    let _ = fs::remove_dir_all(&root);
    let (unset, set) = (unset?, set?);

    assert!(set.contains("--base origin/release --head HEAD"), "{set}");
    assert!(set.contains("\n- Diff SARIF: "), "{set}");
    assert!(set.contains("\n- Repo seam SARIF: "), "{set}");
    assert!(!set.contains("disabled by `RIPR_UPLOAD_SARIF`"), "{set}");
    assert!(set.contains("\n- Mode: `inline`"), "{set}");

    // #7368: with no base anywhere in the environment the summary must name
    // the missing base, not invent `origin/main` for a repository whose
    // default branch may be anything else.
    assert!(
        !unset.contains("--base origin/"),
        "no base may be invented: {unset}"
    );
    assert!(
        unset.contains("no base is derivable from the event payload"),
        "{unset}"
    );
    assert!(
        unset.contains("\n- SARIF upload: disabled by `RIPR_UPLOAD_SARIF`"),
        "{unset}"
    );
    assert!(unset.contains("\n- Mode: `off`"), "{unset}");
    Ok(())
}

/// A static-limitation card is a note, not a defect: the real
/// `reports ci-packet` annotation step emits it as `::notice`, while the card
/// that names a gap keeps `::warning` (#11142).
#[test]
fn ci_packet_annotations_emit_a_notice_for_a_static_limitation() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-ci-packet-notice-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("target/ripr/review"))?;
    fs::write(
        root.join("target/ripr/review/comments.json"),
        r#"{"comments":[
            {"id":"limit","gap_state":"static_limitation","reason":"RIPR has no test to suggest here.","placement":{"path":"src/parse.rs","line":2}},
            {"id":"gap","gap_state":"actionable","reason":"No test checks the boundary.","placement":{"path":"src/pricing.rs","line":4}}
        ],"summary_only":[],"suppressed":[]}"#,
    )?;

    let output = run_ripr_in(
        &root,
        &[
            "reports",
            "ci-packet",
            "--root",
            ".",
            "--step",
            "Emit RIPR PR guidance annotations",
        ],
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let _ = fs::remove_dir_all(&root);

    let notices: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("::notice "))
        .collect();
    let warnings: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("::warning "))
        .collect();
    assert!(output.status.success(), "{stdout}");
    assert_eq!(notices.len(), 1, "{stdout}");
    assert!(notices[0].contains("file=src/parse.rs,line=2"), "{stdout}");
    assert_eq!(warnings.len(), 1, "{stdout}");
    assert!(
        warnings[0].contains("file=src/pricing.rs,line=4"),
        "{stdout}"
    );
    Ok(())
}

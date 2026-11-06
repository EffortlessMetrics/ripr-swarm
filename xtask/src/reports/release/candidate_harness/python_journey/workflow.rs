use super::*;

pub(super) fn follow(
    root: &Path,
    foreign: &Path,
    evidence: &Path,
    history: &Value,
    git: &mut impl FnMut(&Path, &[&str], Option<&str>) -> Result<CommandResult, String>,
    run: &mut impl FnMut(&[String], &Path, &str) -> Result<CommandResult, String>,
) -> Result<Value, String> {
    let root_arg = root.to_str().ok_or("Python root UTF-8")?;
    let mut commands = Vec::new();
    for args in [
        vec!["doctor", "--help"],
        vec!["doctor", "--root", root_arg, "--json"],
    ] {
        let result = invoke(
            run,
            foreign,
            evidence,
            &mut commands,
            strings(&args),
            "Python public doctor",
        )?;
        require_success(&result, "Python doctor")?;
        if args.contains(&"--json") {
            let report: Value = serde_json::from_str(&result.stdout).map_err(|e| e.to_string())?;
            if report["tool"] != "ripr"
                || report["root"] != root_arg
                || report["checks"].as_array().is_none_or(Vec::is_empty)
            {
                return Err("Python doctor lost selected-root nonempty diagnostics".into());
            }
        }
    }
    let base = history["base"].as_str().ok_or("Python base pin missing")?;
    let checked = invoke(
        run,
        foreign,
        evidence,
        &mut commands,
        strings(&["check", "--root", root_arg, "--base", base]),
        "Python public diagnosis",
    )?;
    require_success(&checked, "Python check")?;
    let action = runtime::verify_action(&checked.stdout, root)?;
    if !checked.stdout.contains("units == 20") {
        return Err("Python diagnosis lost equality discriminator".into());
    }
    let start = checked
        .stdout
        .lines()
        .find_map(|line| {
            let offset = line.find("ripr agent repair --gap-id ")?;
            Some(line[offset..].trim().trim_matches('`').to_string())
        })
        .ok_or("Python check printed no gap start")?;
    let before_args = printed_words::printed_words(&start)?;
    let gap = flag(&before_args, "--gap-id")?;
    if before_args
        != strings(&[
            "ripr", "agent", "repair", "--gap-id", &gap, "--base", base, "--root", root_arg,
            "--phase", "before",
        ])
        || gap
            != "gap:python:pricing/core.py:bulk_discount_rate:predicate_boundary:predicate:units>=20"
    {
        return Err(format!(
            "Python emitted start bound another subject: {start}"
        ));
    }
    let caller_receipt =
        receipts::first_pr(root, foreign, evidence, base, &gap, &mut commands, run)?;
    let not_run_receipt = receipts::caller_statuses(
        &caller_receipt,
        root,
        evidence,
        &mut commands,
        run,
        &["not_run"],
    )?;
    for args in [
        vec!["agent", "verify-execute", "--help"],
        vec!["agent", "receipt", "--help"],
        vec!["receipt", "write", "--help"],
    ] {
        let help = invoke(
            run,
            foreign,
            evidence,
            &mut commands,
            strings(&args),
            "Python receipt/verification authority help",
        )?;
        require_success(&help, "Python authority help")?;
    }
    // The caller supplies the publicly documented project environment. Never
    // activate or install it here, nor repair import paths after a failure.
    let environment = runtime::identity(evidence, foreign)?;
    let selected = runtime::record(evidence, foreign, &action, "printed-selected")?;
    runtime::passed(&selected, 1, "literal Python printed selector")?;
    let bare = action.replacen("python -m pytest ", "pytest ", 1);
    let rejected = runtime::record(evidence, foreign, &bare, "bare-control")?;
    if rejected.status != Some(4)
        || !(rejected.stdout.clone() + &rejected.stderr).contains("ModuleNotFoundError")
        || !(rejected.stdout.clone() + &rejected.stderr).contains("pricing")
    {
        return Err(
            "bare pytest failed to discriminate the clean flat-layout import environment".into(),
        );
    }
    let before = invoke(
        run,
        foreign,
        evidence,
        &mut commands,
        before_args[1..].to_vec(),
        "Python literal before",
    )?;
    require_success(&before, "Python emitted before")?;
    let packet = read_json(&root.join("target/ripr/workflow/gap-repair-packet.json"))?;
    let subjects = packet["packets"]
        .as_array()
        .ok_or("Python packet subjects missing")?;
    if subjects.len() != 1
        || subjects[0]["canonical_gap_id"] != gap
        || subjects[0]["allowed_edit_surface"] != json!([TARGET])
        || subjects[0]["recommended_test"]["name"] != "test_above_boundary_gets_discount"
    {
        return Err("Python card no longer selects the retained existing test".into());
    }
    // Retain producer output as-is. The Python gap packet currently exposes
    // strings, without a typed executable runtime spec; never invent one.
    write_json(&evidence.join("producer-packet.json"), &packet)?;
    let continuation = before
        .stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Next, after the test edit: "))
        .or_else(|| {
            before
                .stderr
                .lines()
                .find_map(|line| line.strip_prefix("ripr: attempt next command: "))
        })
        .ok_or("Python before printed no continuation")?;
    let next = printed_words::printed_words(continuation)?;
    let attempt = flag(&next, "--attempt")?;
    if next
        != strings(&[
            "ripr",
            "continue",
            "--root",
            root_arg,
            "--attempt",
            &attempt,
        ])
        || attempt
            .strip_prefix("repair-attempt-")
            .is_none_or(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Python printed continuation is unsafe or not root-bound".into());
    }
    let attempt_path = root.join(format!(
        "target/ripr/repair-attempts/{attempt}/attempt.json"
    ));
    let manifest = read_json(&attempt_path)?;
    if manifest["state"] != "awaiting_edit"
        || manifest["root"] != root_arg
        || manifest["seam_id"] != gap
    {
        return Err("Python attempt state/root/subject differs".into());
    }
    let before_artifacts = validate_artifacts(root, &manifest)?;
    let original_test = plain_bytes(&root.join(TARGET))?;
    let mut edited = original_test.clone();
    edited.extend_from_slice(EDIT.as_bytes());
    fs::write(root.join(TARGET), &edited).map_err(|e| e.to_string())?;
    let changed = git_ok(git, root, &["diff", "--name-only"], None)?.stdout;
    if changed.trim() != TARGET {
        return Err(format!("Python repair escaped selected test: {changed}"));
    }
    // Exercise the exact suggested test after strengthening it. The whole
    // module run remains a separately named public project verification.
    let printed_after = runtime::record(evidence, foreign, &action, "printed-after-edit")?;
    runtime::passed(&printed_after, 1, "printed Python post-edit selector")?;
    let edited_text = String::from_utf8(edited.clone()).map_err(|e| e.to_string())?;
    let wrong_test = edited_text.replace(
        "bulk_discount_rate(21) == 0.15",
        "bulk_discount_rate(21) == 0.0",
    );
    if wrong_test == edited_text {
        return Err("focused Python failure control lost its subject".into());
    }
    fs::write(root.join(TARGET), wrong_test).map_err(|e| e.to_string())?;
    let focused_failure = runtime::record(evidence, foreign, &action, "focused-failure-control");
    fs::write(root.join(TARGET), &edited).map_err(|e| e.to_string())?;
    let focused_failure = focused_failure?;
    if focused_failure.status != Some(1)
        || !focused_failure.stdout.contains("1 failed")
        || !focused_failure.stdout.contains("AssertionError")
    {
        return Err("literal Python focus failed to expose a failing test".into());
    }
    let project_script = "python -m pytest tests/test_pricing.py";
    let project = runtime::record(evidence, root, project_script, "project-after-edit")?;
    runtime::passed(&project, 2, "documented Python project verification")?;
    let source = root.join("pricing/core.py");
    let original_source = String::from_utf8(plain_bytes(&source)?).map_err(|e| e.to_string())?;
    if original_source.matches("units >= 20").count() != 1 {
        return Err("Python wrong-source control no longer discriminates".into());
    }
    fs::write(
        &source,
        original_source.replacen("units >= 20", "units > 20", 1),
    )
    .map_err(|e| e.to_string())?;
    let wrong = runtime::record(evidence, foreign, &action, "wrong-source-control");
    fs::write(&source, &original_source).map_err(|e| e.to_string())?;
    let wrong = wrong?;
    if wrong.status != Some(1)
        || !wrong.stdout.contains("1 failed")
        || !wrong.stdout.contains("AssertionError")
    {
        return Err("Python equality oracle accepted the wrong boundary".into());
    }
    let restored = runtime::record(evidence, foreign, &action, "restored-source")?;
    runtime::passed(&restored, 1, "restored literal Python verification")?;
    git_ok(git, root, &["add", TARGET], None)?;
    git_ok(
        git,
        root,
        &[
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-m",
            "repair: equality boundary test",
        ],
        Some("2026-10-03T00:03:00Z"),
    )?;
    let edit_head = git_ok(git, root, &["rev-parse", "HEAD"], None)?
        .stdout
        .trim()
        .to_string();
    let after = invoke(
        run,
        foreign,
        evidence,
        &mut commands,
        next[1..].to_vec(),
        "Python literal continuation",
    )?;
    require_success(&after, "Python after/static phase")?;
    if !after.stdout.contains("closed: weakly_exposed -> exposed")
        || !after.stdout.contains("Test run: none recorded")
    {
        return Err("Python after conflates or loses static/runtime state".into());
    }
    let verify_path = root.join("target/ripr/workflow/agent-verify.json");
    let verify = read_json(&verify_path)?;
    let receipt_path = root.join("target/ripr/reports/agent-receipt.json");
    let receipt = read_json(&receipt_path)?;
    if verify["bound_gap"]["closed"] != true
        || receipt["bound_gap"]["closed"] != true
        || receipt["verification"]["status"] != "verification_not_run"
        || receipt["verification"]["commands_run"] != json!([])
    {
        return Err("Python static receipt promoted an external project run".into());
    }
    let receipt_controls =
        receipts::static_refusals(root, foreign, evidence, &gap, &attempt, &mut commands, run)?;
    // Temporal staleness is distinct from edited artifact bytes. Retain the
    // later commit in this disposable fixture, then detach to the verified
    // edit commit before consuming positive receipts again.
    let workflow_before_stale = inventory(&root.join("target/ripr/workflow"))?;
    let mut later_test = edited.clone();
    later_test.extend_from_slice(b"\n# later test revision for stale-artifact control\n");
    fs::write(root.join(TARGET), later_test).map_err(|e| e.to_string())?;
    git_ok(git, root, &["add", TARGET], None)?;
    git_ok(
        git,
        root,
        &[
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-m",
            "control: later test revision",
        ],
        Some("2026-10-03T00:04:00Z"),
    )?;
    let stale_head = git_ok(git, root, &["rev-parse", "HEAD"], None)?
        .stdout
        .trim()
        .to_string();
    let stale_out = "target/ripr/reports/python-temporally-stale-control.json";
    let stale = invoke(
        run,
        foreign,
        evidence,
        &mut commands,
        strings(&[
            "agent",
            "receipt",
            "--root",
            root_arg,
            "--verify-json",
            verify_path.to_str().ok_or("verify UTF-8")?,
            "--seam-id",
            &gap,
            "--attempt",
            &attempt,
            "--json",
            "--out",
            stale_out,
        ]),
        "Python temporally stale verify control",
    );
    git_ok(
        git,
        root,
        &["checkout", "--quiet", "--detach", &edit_head],
        None,
    )?;
    let stale = stale?;
    if stale.success
        || !stale.stderr.contains("not_canonical")
        || !stale.stderr.contains("rerun agent verify")
        || root.join(stale_out).exists()
        || inventory(&root.join("target/ripr/workflow"))? != workflow_before_stale
        || plain_bytes(&root.join(TARGET))? != edited
    {
        return Err(format!(
            "Python temporal stale control lost refusal, recovery or restoration: {}",
            stale.stderr
        ));
    }
    let caller_states = receipts::caller_statuses(
        &caller_receipt,
        root,
        evidence,
        &mut commands,
        run,
        &["failed", "passed"],
    )?;
    Ok(
        json!({"commands":commands,"environment":environment,"gap_id":gap,"attempt_id":attempt,
        "emitted_before":start,"emitted_continue":continuation,"literal_verify_action":action,
        "before_artifacts":before_artifacts,"packet":packet,"attempt_manifest":artifact(&attempt_path)?,
        "typed_project_execution_spec":{"state":"absent","surface":"gap-bound Python packet","execution_authority":"not_granted_by_this_packet"},
        "test_edit":{"path":TARGET,"before_sha256":digest(&original_test),"after_sha256":digest(&edited),"commit":edit_head,"bounded_diff":changed},
        "project_verification":{"source":"retained fixture manifest and public module-form guidance","command":project_script,"exit":project.status,"tests_passed":2,
            "printed_selector_tests_passed":1,"printed_selector_exercises_added_equality":true,
            "operator_edit":"strengthen the exact suggested existing test as the live card requests; no new test name or changed printed selector"},
        "wrong_source_control":{"literal_selector":true,"exit":wrong.status,"failed":1,"restored_passed":1},
        "bare_console_control":{"exit":rejected.status,"import_failure_visible":true},
        "focused_failure_control":{"exit":focused_failure.status,"failed":1},
        "static_movement":{"status":"closed","before":"weakly_exposed","after":"exposed"},
        "producer_receipt":{"artifact":artifact(&receipt_path)?,"runtime_status":"verification_not_run","authority":"static attempt receipt"},
        "producer_verify":artifact(&verify_path)?,
        "static_receipt_controls":receipt_controls,"caller_status_receipts":{"not_run_before_any_tests":not_run_receipt,"after_observed_runs":caller_states},
        "temporal_stale_control":{"exit":stale.status,"stale_head":stale_head,"restored_head":edit_head,
            "output_absent":true,"workflow_unchanged":true,"recovery":"rerun agent verify"},
        "authority_limits":["Ordinary static attempt receipts deliberately report verification_not_run (SPEC-0135); external pytest observations and canonical caller-status receipts remain separate.",
            "Accepted repair-trust selection is a prerequisite to the separately governed Python verify phase (SPEC-0176/#3570); this manual route does not invent one or qualify that separate contract."],
        "remaining_gates":["Central immutable installed candidate execution and final language matrix.","Final installed/blind acceptance and retained controls outside this preparation scope."]}),
    )
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}
fn flag(words: &[String], name: &str) -> Result<String, String> {
    let offsets = words
        .iter()
        .enumerate()
        .filter(|(_, s)| s.as_str() == name)
        .map(|(n, _)| n)
        .collect::<Vec<_>>();
    if offsets.len() != 1 {
        return Err(format!("missing/duplicate {name}"));
    }
    words
        .get(offsets[0] + 1)
        .cloned()
        .ok_or_else(|| format!("missing value {name}"))
}
fn validate_artifacts(root: &Path, manifest: &Value) -> Result<Vec<Value>, String> {
    let rows = manifest["artifacts"]
        .as_array()
        .filter(|r| !r.is_empty())
        .ok_or("Python before artifacts missing")?;
    for row in rows {
        let path = relative_path(
            root,
            row["path"].as_str().ok_or("Python artifact path missing")?,
        )?;
        let bytes = plain_bytes(&path)?;
        if row["bytes"].as_u64() != Some(bytes.len() as u64)
            || row["sha256"] != format!("sha256:{}", digest(&bytes))
        {
            return Err("Python producer artifact digest/length differs".into());
        }
    }
    for role in [
        "before_check_findings",
        "agent_packet",
        "edit_cage_baseline",
    ] {
        if rows.iter().filter(|r| r["role"] == role).count() != 1 {
            return Err(format!("missing unique Python artifact {role}"));
        }
    }
    Ok(rows.clone())
}

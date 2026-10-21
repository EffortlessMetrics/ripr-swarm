use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use super::assess::{Observed, assess};
use super::command::{Options, aggregate, build_packet, family_counts, packet_digest, parse_args};
use super::corpus::{Case, Expect, Route, all_cases};
use super::model::{
    Aggregate, Control, ControlKind, Family, Packet, Role, Row, RowResult, Subjects, render_json,
    render_markdown,
};

type TestResult = Result<(), String>;

fn case_named(id: &str) -> Result<Case, String> {
    all_cases()
        .into_iter()
        .find(|case| case.id == id)
        .ok_or_else(|| format!("no corpus case named {id}"))
}

fn observe<'a>(stdout: &'a str, stderr: &'a str, artifact: Option<&'a str>) -> Observed<'a> {
    Observed {
        exit_code: Some(0),
        stdout,
        stderr,
        artifact,
    }
}

fn check_json(findings: &[&str], limitations: &str, complete: bool) -> String {
    let findings = findings
        .iter()
        .map(|file| format!("{{\"canonical_gap\":{{\"file\":\"{file}\"}}}}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"findings\":[{findings}],\"analysis_outcome\":{{\"analysis_complete\":{complete},\"outcome\":{{\"limitations\":[{limitations}]}}}}}}"
    )
}

fn python_limitation(path: &str, detail: &str, kind: &str) -> String {
    format!("{{\"kind\":\"{kind}\",\"path\":\"{path}\",\"bounded_detail\":\"{detail}\"}}")
}

#[test]
fn corpus_ids_are_unique_and_every_family_has_a_discriminator() -> TestResult {
    let cases = all_cases();
    let ids: BTreeSet<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    if ids.len() != cases.len() {
        return Err("duplicate corpus case ids".to_string());
    }
    for family in Family::ALL {
        let discriminators = cases
            .iter()
            .filter(|case| case.family == family && case.role == Role::Discriminator)
            .count();
        if discriminators == 0 {
            return Err(format!("{} has no discriminator row", family.as_str()));
        }
    }
    Ok(())
}

#[test]
fn corpus_covers_every_issue_required_shape() -> TestResult {
    let cases = all_cases();
    let shapes: BTreeSet<&str> = cases.iter().map(|case| case.shape).collect();
    for required in [
        "nested_blocks",
        "nested_if",
        "parentheses",
        "closures",
        "prefix_operators",
        "generic_arguments",
        "else_if_chain",
        "binary_operator_chain",
        "binary_operator_run",
        "ternary_run",
        "unary_not_run",
        "elif_chain",
        "deep_but_supported",
        "comment_string_operator_elif_lookalikes",
    ] {
        if !shapes.contains(required) {
            return Err(format!("corpus lacks required shape {required}"));
        }
    }
    Ok(())
}

#[test]
fn rust_cases_exercise_both_routes_and_the_pilot_reads_an_artifact() -> TestResult {
    let check = case_named("rust-nested-blocks-over-budget.check")?;
    let pilot = case_named("rust-nested-blocks-over-budget.pilot")?;
    if check.route != Route::Check || pilot.route != Route::Pilot {
        return Err("route mismatch".to_string());
    }
    if pilot.artifact.is_none() {
        return Err("pilot case must read its repo-exposure artifact".to_string());
    }
    if !matches!(pilot.expect, Expect::TypedLimitation { .. }) {
        return Err("pilot over-budget row must expect a typed limitation".to_string());
    }
    Ok(())
}

#[test]
fn corpus_inputs_are_deterministic_and_the_hostile_file_is_outside_the_diff() -> TestResult {
    let first = all_cases();
    let second = all_cases();
    for (left, right) in first.iter().zip(&second) {
        if left.files != right.files {
            return Err(format!("{} generated different bytes twice", left.id));
        }
        let diff = left
            .files
            .iter()
            .find(|(path, _)| path == super::corpus::DIFF_FILE)
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).to_string())
            .ok_or_else(|| format!("{} has no diff", left.id))?;
        if let Some(hostile) = &left.hostile
            && diff.contains(hostile.as_str())
        {
            return Err(format!("{} names its hostile file in the diff", left.id));
        }
        if !diff.contains(left.changed.as_str()) {
            return Err(format!("{} diff does not change {}", left.id, left.changed));
        }
    }
    Ok(())
}

#[test]
fn python_typed_limitation_passes_only_with_all_evidence() -> TestResult {
    let case = case_named("python-elif-chain-over-budget")?;
    let limitation = python_limitation(
        "src/deep.py",
        "parse_budget: syntax tree depth estimate exceeded 256",
        "language_scope_unsupported",
    );
    let good = check_json(&["src/app.py"], &limitation, false);
    let result = assess(&case, &observe(&good, "", None));
    if result.result != RowResult::PassedTypedLimitation {
        return Err(format!("good output classified {:?}", result.result));
    }
    if result.declined_files != 1 || result.findings_reported != 1 {
        return Err("subject counts were not retained".to_string());
    }

    let clean = check_json(&["src/app.py"], "", true);
    let missing = assess(&case, &observe(&clean, "", None));
    if missing.result != RowResult::MissingOrFalseCleanLimitation {
        return Err(format!("false clean classified {:?}", missing.result));
    }

    let still_complete = check_json(&["src/app.py"], &limitation, true);
    let incomplete = assess(&case, &observe(&still_complete, "", None));
    if incomplete.result != RowResult::MissingOrFalseCleanLimitation {
        return Err(format!(
            "limitation with complete=true classified {:?}",
            incomplete.result
        ));
    }

    let lost = check_json(&[], &limitation, false);
    let wrong = assess(&case, &observe(&lost, "", None));
    if wrong.result != RowResult::WrongSubject {
        return Err(format!(
            "lost changed finding classified {:?}",
            wrong.result
        ));
    }

    let other_class = python_limitation("src/deep.py", "parse_budget: x", "some_other_kind");
    let drifted = check_json(&["src/app.py"], &other_class, false);
    if assess(&case, &observe(&drifted, "", None)).result != RowResult::SemanticFailure {
        return Err("a different limitation class must not pass".to_string());
    }

    let not_json = assess(&case, &observe("ripr: ok", "", None));
    if not_json.result != RowResult::SemanticFailure {
        return Err(format!("non-JSON classified {:?}", not_json.result));
    }
    Ok(())
}

#[test]
fn supported_rows_reject_a_limitation_that_names_the_input() -> TestResult {
    let case = case_named("python-supported-control")?;
    let limitation = python_limitation(
        "src/deep.py",
        "parse_budget: nesting depth exceeded 128",
        "language_scope_unsupported",
    );
    let over = check_json(&["src/app.py"], &limitation, false);
    if assess(&case, &observe(&over, "", None)).result != RowResult::SemanticFailure {
        return Err("a false limit on a supported file must fail".to_string());
    }
    let clean = check_json(&["src/app.py"], "", true);
    if assess(&case, &observe(&clean, "", None)).result != RowResult::PassedSupportedAnalysis {
        return Err("a clean supported run must pass".to_string());
    }
    let empty = check_json(&[], "", true);
    if assess(&case, &observe(&empty, "", None)).result != RowResult::WrongSubject {
        return Err("a supported run that lost the changed finding must fail".to_string());
    }
    Ok(())
}

#[test]
fn rust_pilot_requires_the_stderr_disclosure_and_the_unaffected_seam() -> TestResult {
    let case = case_named("rust-parentheses-over-budget.pilot")?;
    let summary = "{\"status\":\"complete\"}";
    let artifact = "{\"run_status\":\"complete\",\"seams\":[{\"seam_id\":\"a\",\"file\":\"src/lib.rs\",\"grip_class\":\"strongly_gripped\"}]}";
    let disclosure = "ripr: src/deep.rs: static limit rust_nesting_budget: estimated nesting depth 10001 exceeds the 256-level parse budget";
    let good = assess(&case, &observe(summary, disclosure, Some(artifact)));
    if good.result != RowResult::PassedTypedLimitation {
        return Err(format!("good pilot classified {:?}", good.result));
    }
    if good.artifact_projection.is_none() {
        return Err("pilot must project its artifact deterministically".to_string());
    }

    let silent = assess(&case, &observe(summary, "", Some(artifact)));
    if silent.result != RowResult::MissingOrFalseCleanLimitation {
        return Err(format!("silent pilot classified {:?}", silent.result));
    }

    let no_seams = "{\"run_status\":\"complete\",\"seams\":[]}";
    let lost = assess(&case, &observe(summary, disclosure, Some(no_seams)));
    if lost.result != RowResult::WrongSubject {
        return Err(format!("lost seam classified {:?}", lost.result));
    }

    let control = case_named("rust-supported-control.pilot")?;
    let false_limit = assess(&control, &observe(summary, disclosure, Some(artifact)));
    if false_limit.result != RowResult::SemanticFailure {
        return Err(format!(
            "false disclosure on a supported file classified {:?}",
            false_limit.result
        ));
    }
    Ok(())
}

#[test]
fn glob_rows_cannot_pass_by_suppressing_or_excluding_silently() -> TestResult {
    let nonmatch = case_named("glob-suppression-many-star-nonmatch")?;
    let visible = format!(
        "{{\"findings\":[{{\"canonical_gap\":{{\"file\":\"{}\"}}}}],\"suppression_policy\":{{\"suppressed\":0}},\"summary\":{{\"suppressed_by_policy\":0}}}}",
        nonmatch.changed
    );
    if assess(&nonmatch, &observe(&visible, "", None)).result != RowResult::PassedSupportedAnalysis
    {
        return Err("a visible finding after a nonmatch must pass".to_string());
    }
    let suppressed = visible.replace("\"suppressed\":0", "\"suppressed\":1");
    if assess(&nonmatch, &observe(&suppressed, "", None)).result != RowResult::SemanticFailure {
        return Err("a nonmatching pattern that suppressed must fail".to_string());
    }

    let generated = case_named("glob-generated-many-star-nonmatch")?;
    let skipped = format!(
        "{{\"findings\":[],\"analysis_outcome\":{{\"analysis_complete\":false,\"outcome\":{{\"limitations\":[{{\"kind\":\"language_scope_unsupported\",\"bounded_detail\":\"1 generated or vendored Rust file(s) were intentionally skipped: {}\"}}]}}}}}}",
        generated.changed
    );
    if assess(&generated, &observe(&skipped, "", None)).result != RowResult::SemanticFailure {
        return Err("a nonmatching generated pattern that excluded the file must fail".to_string());
    }
    let excluded = case_named("glob-generated-star-in-segment")?;
    let silent = check_json(&[], "", true);
    if assess(&excluded, &observe(&silent, "", None)).result
        != RowResult::MissingOrFalseCleanLimitation
    {
        return Err("an exclusion with no disclosure must not pass".to_string());
    }
    Ok(())
}

#[test]
fn a_malformed_selector_passes_only_as_a_policy_refusal() -> TestResult {
    let case = case_named("glob-suppression-empty-pattern-is-a-policy-error")?;
    let refused =
        "{\"analysis_scope\":{\"limitation\":\"suppression_policy_invalid\"},\"findings\":[]}";
    let mut exit_two = observe(refused, "", None);
    exit_two.exit_code = Some(2);
    if assess(&case, &exit_two).result != RowResult::PassedTypedLimitation {
        return Err("a refused policy at exit 2 must pass".to_string());
    }
    if assess(&case, &observe(refused, "", None)).result != RowResult::MissingOrFalseCleanLimitation
    {
        return Err("a policy refusal that exits 0 must not pass".to_string());
    }
    let clean = check_json(&["src/lib.rs"], "", true);
    if assess(&case, &observe(&clean, "", None)).result != RowResult::MissingOrFalseCleanLimitation
    {
        return Err("treating a malformed selector as a clean run must not pass".to_string());
    }
    Ok(())
}

fn row(id: &str, family: Family, result: RowResult) -> Row {
    Row {
        id: id.to_string(),
        family,
        role: Role::Discriminator,
        shape: "shape".to_string(),
        expected: "expected".to_string(),
        result,
        detail: "detail".to_string(),
        argv: vec!["check".to_string()],
        cwd: "<case-root>".to_string(),
        deadline_seconds: 60,
        termination: "exit_code 0".to_string(),
        limitation: None,
        inputs: Vec::new(),
        subjects: Subjects {
            workspace_files: 4,
            unchanged_outside_diff: 1,
            findings_reported: 1,
            findings_suppressed: 0,
            declined_files: 0,
        },
        stdout_sha256: "a".repeat(64),
        stderr_sha256: "b".repeat(64),
        artifact_sha256: None,
        cleanup: "removed".to_string(),
    }
}

fn control(id: &str, observed: Option<&str>, discriminated: bool) -> Control {
    Control {
        id: id.to_string(),
        kind: ControlKind::Removal,
        family: None,
        expected: "fails".to_string(),
        observed: observed.map(str::to_string),
        discriminated,
        detail: String::new(),
    }
}

fn options() -> Options {
    Options {
        binary: PathBuf::from("ripr"),
        out: PathBuf::from("out"),
        work: PathBuf::from("work"),
        expected_sha256: None,
        expected_version: None,
        families: Vec::new(),
        removal_controls: Vec::new(),
        repeat: true,
        deadline: Duration::from_mins(1),
        allow_incomplete: false,
    }
}

fn all_family_rows(result: RowResult) -> Vec<Row> {
    Family::ALL
        .iter()
        .map(|family| row(&format!("row-{}", family.as_str()), *family, result))
        .collect()
}

fn cases_for_all_families() -> Vec<Case> {
    let mut seen = BTreeSet::new();
    all_cases()
        .into_iter()
        .filter(|case| seen.insert(case.family))
        .collect()
}

fn agg(
    rows: &[Row],
    cases: &[Case],
    controls: &[Control],
    cleanup: &str,
) -> (Aggregate, Vec<String>) {
    aggregate(
        rows,
        &family_counts(rows, cases),
        controls,
        &options(),
        cleanup,
    )
}

#[test]
fn aggregate_is_passed_only_when_everything_ran_and_discriminated() -> TestResult {
    let cases = cases_for_all_families();
    let rows = all_family_rows(RowResult::PassedSupportedAnalysis);
    let controls = vec![control("c", Some("process_abort"), true)];
    let (aggregate, reasons) = agg(&rows, &cases, &controls, "removed");
    if aggregate != Aggregate::Passed || !reasons.is_empty() {
        return Err(format!("expected passed, got {aggregate:?} {reasons:?}"));
    }

    // A missing control makes the packet incomplete, not green.
    let missing = vec![control("c", None, false)];
    let (aggregate, _) = agg(&rows, &cases, &missing, "removed");
    if aggregate != Aggregate::Incomplete {
        return Err(format!("missing control gave {aggregate:?}"));
    }

    // A control that ran and did not discriminate is a failure.
    let weak = vec![control("c", Some("passed_typed_limitation"), false)];
    let (aggregate, _) = agg(&rows, &cases, &weak, "removed");
    if aggregate != Aggregate::Failed {
        return Err(format!("non-discriminating control gave {aggregate:?}"));
    }

    // One failing row fails the packet even when every control is green.
    let mut failing = all_family_rows(RowResult::PassedSupportedAnalysis);
    failing[1].result = RowResult::NonterminalDeadline;
    let (aggregate, reasons) = agg(&failing, &cases, &controls, "removed");
    if aggregate != Aggregate::Failed
        || !reasons
            .iter()
            .any(|reason| reason.contains("nonterminal_deadline"))
    {
        return Err(format!("failing row gave {aggregate:?} {reasons:?}"));
    }

    // A watchdog kill is never a pass.
    if RowResult::NonterminalDeadline.is_pass() || RowResult::NotRun.is_pass() {
        return Err("only the two passing states may pass".to_string());
    }
    Ok(())
}

#[test]
fn zero_rows_or_an_unexecuted_family_cannot_aggregate_to_pass() -> TestResult {
    let cases = cases_for_all_families();
    let (aggregate, _) = agg(&[], &[], &[], "removed");
    if aggregate != Aggregate::Failed {
        return Err(format!("zero rows gave {aggregate:?}"));
    }
    let mut rows = all_family_rows(RowResult::PassedSupportedAnalysis);
    rows[0].result = RowResult::NotRun;
    let (aggregate, reasons) = agg(&rows, &cases, &[], "removed");
    if aggregate != Aggregate::Failed
        || !reasons
            .iter()
            .any(|reason| reason.contains("executed zero rows"))
    {
        return Err(format!("unexecuted family gave {aggregate:?} {reasons:?}"));
    }
    let (aggregate, _) = agg(
        &all_family_rows(RowResult::PassedSupportedAnalysis),
        &cases,
        &[],
        "retained (busy)",
    );
    if aggregate != Aggregate::Failed {
        return Err(format!("failed cleanup gave {aggregate:?}"));
    }
    Ok(())
}

fn packet(rows: Vec<Row>, elapsed_ms: u128, path: &str) -> Result<Packet, String> {
    use super::model::{BinaryIdentity, CORPUS_SCHEMA, HostIdentity, Observation, SCHEMA};
    let mut packet = Packet {
        schema: SCHEMA,
        corpus_schema: CORPUS_SCHEMA,
        binary: BinaryIdentity {
            path: path.to_string(),
            bytes: 10,
            sha256: "c".repeat(64),
            reported_version: "ripr 0.0.0".to_string(),
            path_resolution: "absolute".to_string(),
        },
        host: HostIdentity {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            family: "unix".to_string(),
        },
        environment_boundary: vec!["boundary".to_string()],
        families: Vec::new(),
        rows,
        controls: vec![control("removal-x", Some("process_abort"), true)],
        aggregate: Aggregate::Passed,
        aggregate_reasons: Vec::new(),
        non_claims: vec!["claim".to_string()],
        digest: String::new(),
        observations: vec![Observation {
            id: "row-rust_nesting".to_string(),
            elapsed_ms,
        }],
    };
    packet.digest = packet_digest(&packet)?;
    Ok(packet)
}

#[test]
fn packet_digest_ignores_timing_and_host_path_but_not_semantic_state() -> TestResult {
    let rows = all_family_rows(RowResult::PassedSupportedAnalysis);
    let fast = packet(rows.clone(), 5, "/a/ripr")?;
    let slow = packet(rows.clone(), 50_000, "/b/other/ripr")?;
    if fast.digest != slow.digest {
        return Err("timing or binary location moved the semantic digest".to_string());
    }
    let mut changed = rows;
    changed[0].result = RowResult::PassedTypedLimitation;
    let different = packet(changed, 5, "/a/ripr")?;
    if different.digest == fast.digest {
        return Err("a changed row state did not move the digest".to_string());
    }
    Ok(())
}

#[test]
fn json_and_markdown_agree_on_every_row_state_and_the_aggregate() -> TestResult {
    let mut rows = all_family_rows(RowResult::PassedSupportedAnalysis);
    rows[2].result = RowResult::ProcessAbort;
    let mut packet = packet(rows, 1, "/a/ripr")?;
    packet.aggregate = Aggregate::Failed;
    let json = render_json(&packet)?;
    let markdown = render_markdown(&packet);
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("packet JSON invalid: {error}"))?;
    let json_rows = value["rows"].as_array().ok_or("rows is not an array")?;
    for json_row in json_rows {
        let id = json_row["id"].as_str().ok_or("row id")?;
        let result = json_row["result"].as_str().ok_or("row result")?;
        let line = markdown
            .lines()
            .find(|line| line.contains(&format!("`{id}`")))
            .ok_or_else(|| format!("markdown lacks row {id}"))?;
        if !line.contains(result) {
            return Err(format!("markdown row {id} does not say {result}"));
        }
    }
    if value["aggregate"].as_str() != Some("failed") || !markdown.contains("Aggregate: **failed**")
    {
        return Err("aggregate disagrees between projections".to_string());
    }
    if !markdown.contains(&packet.digest)
        || value["digest"].as_str() != Some(packet.digest.as_str())
    {
        return Err("digest disagrees between projections".to_string());
    }
    Ok(())
}

#[test]
fn arguments_require_a_binary_and_reject_unknown_flags() -> TestResult {
    let args = |items: &[&str]| {
        items
            .iter()
            .map(|item| (*item).to_string())
            .collect::<Vec<_>>()
    };
    if parse_args(&[]).is_ok() {
        return Err("missing --binary must be refused".to_string());
    }
    if parse_args(&args(&["--binary", "x", "--nope", "1"])).is_ok() {
        return Err("unknown flag must be refused".to_string());
    }
    if parse_args(&args(&["--binary", "x", "--family", "nope"])).is_ok() {
        return Err("unknown family must be refused".to_string());
    }
    if parse_args(&args(&[
        "--binary",
        "x",
        "--removal-control",
        "rust_nesting",
    ]))
    .is_ok()
    {
        return Err("a removal control needs <family>=<binary>".to_string());
    }
    let parsed = parse_args(&args(&[
        "--binary",
        "x",
        "--family",
        "python_nesting",
        "--removal-control",
        "python_nesting=/tmp/old",
        "--no-repeat",
    ]))?;
    if parsed.families != vec![Family::PythonNesting]
        || parsed.removal_controls.len() != 1
        || parsed.repeat
    {
        return Err("flags were not parsed".to_string());
    }
    Ok(())
}

#[cfg(unix)]
mod unix_stubs {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-termination-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|error| format!("mkdir: {error}"))?;
        fs::canonicalize(&root).map_err(|error| format!("canonicalize: {error}"))
    }

    fn stub(root: &std::path::Path, script: &str) -> Result<PathBuf, String> {
        let path = root.join("ripr");
        fs::write(&path, script).map_err(|error| format!("write stub: {error}"))?;
        let mut permissions = fs::metadata(&path)
            .map_err(|error| format!("stat stub: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).map_err(|error| format!("chmod stub: {error}"))?;
        Ok(path)
    }

    fn stub_options(root: &std::path::Path, binary: PathBuf) -> Options {
        Options {
            binary,
            out: root.join("out"),
            work: root.join("work"),
            expected_sha256: None,
            expected_version: None,
            families: vec![Family::PythonNesting],
            removal_controls: Vec::new(),
            repeat: false,
            deadline: Duration::from_secs(5),
            allow_incomplete: false,
        }
    }

    #[test]
    fn a_stub_that_answers_clean_to_everything_cannot_earn_a_pass() -> TestResult {
        let root = scratch("clean")?;
        let binary = stub(
            &root,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'ripr 0.0.0 (stub)'; exit 0; fi\necho '{\"findings\":[{\"canonical_gap\":{\"file\":\"src/app.py\"}}],\"analysis_outcome\":{\"analysis_complete\":true,\"outcome\":{\"limitations\":[]}}}'\n",
        )?;
        let packet = build_packet(&stub_options(&root, binary))?;
        let _ = fs::remove_dir_all(&root);
        if packet.aggregate != Aggregate::Failed {
            return Err(format!("stub earned {:?}", packet.aggregate));
        }
        let false_clean = packet
            .rows
            .iter()
            .filter(|row| row.result == RowResult::MissingOrFalseCleanLimitation)
            .count();
        if false_clean == 0 {
            return Err("no over-budget row recognized the missing limitation".to_string());
        }
        if packet.rows.iter().any(|row| row.cleanup != "removed") {
            return Err("a row retained its workspace".to_string());
        }
        Ok(())
    }

    #[test]
    fn an_aborting_stub_is_a_process_abort_row_and_the_work_root_is_removed() -> TestResult {
        let root = scratch("abort")?;
        let binary = stub(
            &root,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'ripr 0.0.0 (stub)'; exit 0; fi\nkill -ABRT $$\n",
        )?;
        let packet = build_packet(&stub_options(&root, binary))?;
        let leftover = root
            .join("work")
            .read_dir()
            .map(|mut entries| entries.next().is_some());
        let _ = fs::remove_dir_all(&root);
        if packet
            .rows
            .iter()
            .any(|row| row.result != RowResult::ProcessAbort)
        {
            return Err("every row of an aborting binary must be process_abort".to_string());
        }
        if packet.aggregate != Aggregate::Failed {
            return Err(format!("aborting binary earned {:?}", packet.aggregate));
        }
        if leftover.unwrap_or(false) {
            return Err("the owned work root was not removed".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_hanging_stub_is_a_deadline_row_not_a_pass() -> TestResult {
        let root = scratch("hang")?;
        let binary = stub(
            &root,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'ripr 0.0.0 (stub)'; exit 0; fi\nsleep 600\n",
        )?;
        let mut options = stub_options(&root, binary);
        options.deadline = Duration::from_secs(1);
        let packet = build_packet(&options)?;
        let _ = fs::remove_dir_all(&root);
        if packet
            .rows
            .iter()
            .any(|row| row.result != RowResult::NonterminalDeadline)
        {
            return Err("every row of a hanging binary must be nonterminal_deadline".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_binary_with_the_wrong_digest_is_refused_before_any_row_runs() -> TestResult {
        let root = scratch("digest")?;
        let binary = stub(
            &root,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'ripr 0.0.0 (stub)'; exit 0; fi\n",
        )?;
        let mut options = stub_options(&root, binary);
        options.expected_sha256 = Some("0".repeat(64));
        let refused = build_packet(&options);
        let ran_anything = root.join("work").exists();
        let _ = fs::remove_dir_all(&root);
        match refused {
            Err(message)
                if message.contains("does not match --expected-sha256") && !ran_anything =>
            {
                Ok(())
            }
            other => Err(format!("wrong digest was not refused up front: {other:?}")),
        }
    }

    #[test]
    fn instrument_controls_discriminate_every_failure_state() -> TestResult {
        let root = scratch("instrument")?;
        let controls = super::super::controls::instrument_controls(&root, &all_cases());
        let _ = fs::remove_dir_all(&root);
        let mut observed = BTreeSet::new();
        for control in &controls {
            if !control.discriminated {
                return Err(format!(
                    "{} did not discriminate: {}",
                    control.id, control.detail
                ));
            }
            observed.insert(control.observed.clone().unwrap_or_default());
        }
        for state in [
            "process_abort",
            "nonterminal_deadline",
            "missing_or_false_clean_limitation",
            "wrong_subject",
            "semantic_failure",
            "instrument_failure",
        ] {
            if !observed.contains(state) {
                return Err(format!("no instrument control reached {state}"));
            }
        }
        Ok(())
    }
}

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use super::assess::{Observed, assess};
use super::command::{Options, aggregate, family_counts, packet_digest, parse_args};
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
fn a_supported_row_must_report_a_complete_analysis_of_the_changed_file() -> TestResult {
    let case = case_named("python-supported-control")?;
    let incomplete = check_json(&["src/app.py"], "", false);
    if assess(&case, &observe(&incomplete, "", None)).result != RowResult::SemanticFailure {
        return Err("analysis_complete=false must not pass a supported row".to_string());
    }
    let naming = check_json(
        &["src/app.py"],
        &python_limitation(
            "src/app.py",
            "parse_budget: stub",
            "language_scope_unsupported",
        ),
        true,
    );
    if assess(&case, &observe(&naming, "", None)).result != RowResult::SemanticFailure {
        return Err("a limitation naming the changed file must not pass".to_string());
    }
    Ok(())
}

#[test]
fn a_nonzero_exit_distrusts_otherwise_correct_output() -> TestResult {
    let case = case_named("python-supported-control")?;
    let clean = check_json(&["src/app.py"], "", true);
    let mut crashed = observe(&clean, "", None);
    crashed.exit_code = Some(101);
    if assess(&case, &crashed).result != RowResult::SemanticFailure {
        return Err("valid JSON followed by exit 101 must not pass".to_string());
    }
    Ok(())
}

#[test]
fn pilot_disclosure_must_name_the_hostile_file() -> TestResult {
    let case = case_named("rust-parentheses-over-budget.pilot")?;
    let artifact = "{\"run_status\":\"complete\",\"seams\":[{\"seam_id\":\"a\",\"file\":\"src/lib.rs\",\"grip_class\":\"g\"}]}";
    let other = "ripr: src/other.rs: static limit rust_nesting_budget: depth";
    let result = assess(
        &case,
        &observe("{\"status\":\"complete\"}", other, Some(artifact)),
    );
    if result.result != RowResult::MissingOrFalseCleanLimitation {
        return Err(format!(
            "disclosure for another file classified {:?}",
            result.result
        ));
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
    rows[1].result = RowResult::ProcessAbort;
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

use super::*;
use crate::rust_judged_panel::host_run::ValidatedInputDigest;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _result = fs::remove_dir_all(&self.0);
    }
}

struct ProjectionFixture {
    _root: TestRoot,
    subject: PacketSubject,
    case: ValidatedHostCase,
    host: ValidatedHostRun,
}

fn test_root(name: &str) -> Result<TestRoot, String> {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ripr-rust-judged-panel-packet-{name}-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&root)
        .map_err(|error| format!("create packet test root `{}`: {error}", root.display()))?;
    Ok(TestRoot(root))
}

fn subject_file(role: &str, repository_path: &str) -> ReplaySubjectFile {
    ReplaySubjectFile {
        source_path: format!("metrics/subjects/{role}"),
        repository_path: repository_path.to_string(),
        sha256: format!("sha256:{role}"),
    }
}

fn fixture(direction: &str) -> Result<ProjectionFixture, String> {
    let root = test_root(direction)?;
    let materialized_root = root.0.join("materialized");
    fs::create_dir_all(materialized_root.join("src"))
        .map_err(|error| format!("create materialized source: {error}"))?;
    let (
        case_id,
        subject_id,
        owner,
        family,
        producer_family,
        expression,
        class,
        action,
        limit,
        missing,
        recommendation,
    ) = match direction {
        "should_gap" => (
            "rust-boundary-missing-equality-should-gap",
            "boundary-missing-equality",
            "discounted_total",
            "predicate_boundary",
            "predicate",
            "amount >= threshold",
            "weakly_exposed",
            "repair_candidate",
            None,
            vec!["Missing discriminator value: amount == threshold"],
            "Add the equality boundary test.",
        ),
        "should_stay_quiet" => (
            "rust-boundary-exact-equality-should-stay-quiet",
            "boundary-exact-equality",
            "discounted_total",
            "predicate_boundary",
            "predicate",
            "amount >= threshold",
            "exposed",
            "no_action",
            None,
            Vec::new(),
            "",
        ),
        "should_limit" => (
            "rust-macro-wrapped-reach-should-limit",
            "macro-wrapped-reach",
            "normalize_score",
            "return_value",
            "return_value",
            "value.max(1)",
            "no_static_path",
            "inspect_static_limitation",
            Some("rust_macro_wrapped_test_call_unresolved"),
            vec![
                "No relevant oracle was detected",
                "No static test path reaches the changed owner",
                "No strong discriminator was detected",
            ],
            "ripr found no static test path to this change — this is not a coverage assessment. A test may already exercise it through macros, helper-call chains, or integration tests that ripr's static model does not yet trace. If none does, add a co-located test that reaches and observes the changed behavior so a discriminator exists.",
        ),
        other => return Err(format!("unsupported fixture direction `{other}`")),
    };
    let source = if owner == "discounted_total" {
        "pub fn discounted_total(amount: u64, threshold: u64) -> u64 {\n    if amount >= threshold { amount - 10 } else { amount }\n}\n"
    } else {
        "pub fn normalize_score(value: u64) -> u64 {\n    value.max(1)\n}\n"
    };
    fs::write(materialized_root.join("src/lib.rs"), source)
        .map_err(|error| format!("write materialized source: {error}"))?;
    let cargo_toml = subject_file("cargo_toml", "Cargo.toml");
    let cargo_lock = subject_file("cargo_lock", "Cargo.lock");
    let config = subject_file("config", "ripr.toml");
    let source_before = subject_file("source_before", "src/lib.rs");
    let source_after = subject_file("source_after", "src/lib.rs");
    let diff = subject_file("diff", "change.diff");
    let tests = vec![subject_file("test", "tests/case.rs")];
    let subject = PacketSubject {
        case_id: case_id.to_string(),
        subject_id: subject_id.to_string(),
        repository: "synthetic-rust".to_string(),
        expected_direction: direction.to_string(),
        anchor_file: "src/lib.rs".to_string(),
        anchor_line: 2,
        owner: owner.to_string(),
        behavior_family: family.to_string(),
        changed_behavior: expression.to_string(),
        required_discriminator: if direction == "should_limit" {
            "exact normalized return value".to_string()
        } else {
            "amount == threshold".to_string()
        },
        expected_classification: class.to_string(),
        expected_actionability: action.to_string(),
        expected_static_limit_kind: limit.map(ToString::to_string),
        expected_missing: missing.iter().map(ToString::to_string).collect(),
        expected_recommendation: recommendation.to_string(),
        cargo_toml,
        cargo_lock,
        config,
        source_before,
        source_after,
        tests,
        diff,
        expected_base: "base".to_string(),
        expected_head: "head".to_string(),
        expected_tree: "tree".to_string(),
    };
    let expected = expected_inputs(&subject);
    let mut finding = serde_json::json!({
        "id": format!("finding-{direction}"),
        "classification": class,
        "probe": {
            "family": producer_family,
            "file": materialized_root.join("src/lib.rs").display().to_string(),
            "line": 2,
            "expression": expression
        },
        "missing": missing,
        "recommended_next_step": recommendation
    });
    if let Some(kind) = limit {
        finding["static_limit_kind"] = Value::String(kind.to_string());
        finding["static_limitation"] = serde_json::json!({"kind": kind});
    }
    let report = serde_json::json!({
        "analysis_outcome": {
            "analysis_complete": true,
            "outcome": {"kind": "complete_with_findings", "limitations": []}
        },
        "findings": [finding]
    });
    let case = ValidatedHostCase {
        case_id: case_id.to_string(),
        subject_id: subject_id.to_string(),
        expected_direction: direction.to_string(),
        repository_base: "base".to_string(),
        repository_head: "head".to_string(),
        repository_tree: "tree".to_string(),
        argv: vec![
            "check".to_string(),
            "--root".to_string(),
            "<materialized-subject>".to_string(),
            "--base".to_string(),
            "base".to_string(),
            "--mode".to_string(),
            "draft".to_string(),
            "--json".to_string(),
        ],
        mode: "draft".to_string(),
        format: "json".to_string(),
        config_path: subject.config.repository_path.clone(),
        config_sha256: subject.config.sha256.clone(),
        diff_path: subject.diff.source_path.clone(),
        diff_sha256: subject.diff.sha256.clone(),
        executed_diff_identity: "sha256:executed".to_string(),
        subject_inputs: expected
            .iter()
            .map(|input| ValidatedInputDigest {
                role: input.role.clone(),
                source_path: input.source_path.clone(),
                repository_path: input.repository_path.clone(),
                sha256: input.sha256.clone(),
            })
            .collect(),
        disposition: "complete".to_string(),
        analyzer_input_identity: "sha256:executed".to_string(),
        receipt_ref: format!("target/ripr/run/cases/{case_id}/receipt.json"),
        receipt_sha256: "sha256:receipt".to_string(),
        stdout_ref: format!("target/ripr/run/cases/{case_id}/stdout.bin"),
        stdout_sha256: "sha256:stdout".to_string(),
        stderr_ref: format!("target/ripr/run/cases/{case_id}/stderr.bin"),
        stderr_sha256: "sha256:stderr".to_string(),
        stdout: serde_json::to_vec(&report)
            .map_err(|error| format!("serialize fixture report: {error}"))?,
        reported_materialized_root: materialized_root.clone(),
        materialized_root,
    };
    let host = ValidatedHostRun {
        current_ref: "target/ripr/rust-judged-panel/current.json".to_string(),
        current_sha256: "sha256:current".to_string(),
        index_ref: "target/ripr/rust-judged-panel/runs/run/index.json".to_string(),
        index_sha256: "sha256:index".to_string(),
        run_id: "run-a".to_string(),
        source_head: "producer-head".to_string(),
        source_tree: "producer-tree".to_string(),
        cargo_lock_sha256: "sha256:producer-lock".to_string(),
        cargo_toml_sha256: "sha256:producer-manifest".to_string(),
        profile: "dev".to_string(),
        features: vec!["default".to_string()],
        host_target: "test-target".to_string(),
        binary_sha256: "sha256:binary".to_string(),
        binary_version: "ripr 0.12.0".to_string(),
        cases: vec![case.clone()],
    };
    Ok(ProjectionFixture {
        _root: root,
        subject,
        case,
        host,
    })
}

fn report_value(case: &ValidatedHostCase) -> Result<Value, String> {
    let text = std::str::from_utf8(&case.stdout)
        .map_err(|error| format!("decode fixture report: {error}"))?;
    super::super::parse_json_without_duplicate_keys(text)
        .map_err(|error| format!("parse fixture report: {error}"))
}

fn store_report(case: &mut ValidatedHostCase, value: &Value) -> Result<(), String> {
    case.stdout = serde_json::to_vec(value)
        .map_err(|error| format!("serialize changed fixture report: {error}"))?;
    Ok(())
}

#[test]
fn rust_judged_panel_packet_projects_all_three_direction_contracts() -> Result<(), String> {
    let fixtures = ["should_gap", "should_stay_quiet", "should_limit"]
        .into_iter()
        .map(fixture)
        .collect::<Result<Vec<_>, _>>()?;
    let subjects = fixtures
        .iter()
        .map(|value| value.subject.clone())
        .collect::<Vec<_>>();
    let mut host = fixtures[0].host.clone();
    host.cases = fixtures.iter().map(|value| value.case.clone()).collect();
    let before = fs::read(fixtures[0]._root.0.join("materialized/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let packets = validate_projection(3, &subjects, &host, "sha256:manifest", "sha256:subjects")?;
    let after = fs::read(fixtures[0]._root.0.join("materialized/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    if packets.len() != 3
        || before != after
        || fixtures
            .iter()
            .any(|value| value._root.0.join("portable").exists())
    {
        Err("projection was incomplete or mutated its input tree".to_string())
    } else {
        Ok(())
    }
}

#[test]
fn rust_judged_panel_packet_rejects_zero_multiple_and_extra_findings() -> Result<(), String> {
    let mut zero = fixture("should_gap")?;
    let mut report = report_value(&zero.case)?;
    report["findings"] = Value::Array(Vec::new());
    store_report(&mut zero.case, &report)?;
    if project_one(&zero.subject, &zero.case, &zero.host, "m", "s").is_ok() {
        return Err("zero findings were accepted".to_string());
    }
    let mut report = report_value(&fixture("should_gap")?.case)?;
    let finding = report
        .get("findings")
        .and_then(Value::as_array)
        .and_then(|findings| findings.first())
        .cloned()
        .ok_or_else(|| "fixture finding missing".to_string())?;
    report["findings"] = Value::Array(vec![finding.clone(), finding]);
    let mut multiple = fixture("should_gap")?;
    store_report(&mut multiple.case, &report)?;
    if project_one(&multiple.subject, &multiple.case, &multiple.host, "m", "s").is_ok() {
        return Err("multiple findings were accepted".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_suffix_only_probe_path() -> Result<(), String> {
    let mut fixture = fixture("should_gap")?;
    let decoy = fixture._root.0.join("decoy/src");
    fs::create_dir_all(&decoy).map_err(|error| format!("create decoy: {error}"))?;
    fs::write(decoy.join("lib.rs"), "pub fn discounted_total() {}\n")
        .map_err(|error| format!("write decoy: {error}"))?;
    let mut report = report_value(&fixture.case)?;
    report["findings"][0]["probe"]["file"] =
        Value::String(decoy.join("lib.rs").display().to_string());
    store_report(&mut fixture.case, &report)?;
    if project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s").is_ok() {
        return Err("suffix-only probe path was accepted".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_swapped_subject_and_stale_plan() -> Result<(), String> {
    let fixture = fixture("should_gap")?;
    let mut case = fixture.case.clone();
    case.subject_id = "boundary-exact-equality".to_string();
    case.argv[4] = "stale-base".to_string();
    if validate_host_join(&fixture.subject, &case, &fixture.host).is_ok() {
        return Err("swapped subject and stale argv were accepted".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_nearby_gap_and_quiet_zero() -> Result<(), String> {
    let mut gap = fixture("should_gap")?;
    let mut report = report_value(&gap.case)?;
    report["findings"][0]["missing"] =
        serde_json::json!(["Missing discriminator value: amount <= threshold"]);
    store_report(&mut gap.case, &report)?;
    if project_one(&gap.subject, &gap.case, &gap.host, "m", "s").is_ok() {
        return Err("nearby missing-discriminator text was accepted".to_string());
    }
    let mut quiet = fixture("should_stay_quiet")?;
    let mut report = report_value(&quiet.case)?;
    report["findings"] = Value::Array(Vec::new());
    store_report(&mut quiet.case, &report)?;
    if project_one(&quiet.subject, &quiet.case, &quiet.host, "m", "s").is_ok() {
        return Err("quiet zero-findings output was accepted".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_limit_as_process_or_analysis_limitation() -> Result<(), String>
{
    let mut fixture = fixture("should_limit")?;
    fixture.case.disposition = "timed_out".to_string();
    if project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s").is_ok() {
        return Err("process timeout was accepted as a static limit".to_string());
    }
    fixture.case.disposition = "complete".to_string();
    let mut report = report_value(&fixture.case)?;
    report["analysis_outcome"]["outcome"]["limitations"] =
        serde_json::json!([{"kind":"timed_out"}]);
    store_report(&mut fixture.case, &report)?;
    if project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s").is_ok() {
        return Err("analysis-level limitation was accepted as a finding limit".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_semantic_identity_excludes_host_provenance() -> Result<(), String> {
    let fixture = fixture("should_gap")?;
    let first = project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    let mut relocated = fixture.host.clone();
    relocated.run_id = "run-b".to_string();
    relocated.host_target = "different-target".to_string();
    relocated.binary_sha256 = "sha256:different-binary".to_string();
    relocated.current_ref = "target/ripr/elsewhere/current.json".to_string();
    let second = project_one(&fixture.subject, &fixture.case, &relocated, "m", "s")?;
    if first.semantic_sha256 != second.semantic_sha256
        || first.host_evidence.run_id == second.host_evidence.run_id
        || first.host_evidence.binary_sha256 == second.host_evidence.binary_sha256
    {
        return Err("host provenance did not remain distinct".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_self_digest_and_direction_tamper() -> Result<(), String> {
    let fixture = fixture("should_gap")?;
    let (subject, case, host) = (&fixture.subject, &fixture.case, &fixture.host);
    let rejects =
        |packet: &PortablePacket| validate_packet(packet, subject, case, host, "m", "s").is_err();
    let mut packet = project_one(subject, case, host, "m", "s")?;
    packet.semantic.observed.recommendation.clear();
    if !rejects(&packet) {
        return Err("semantic self-digest tamper was accepted".to_string());
    }
    packet.semantic_sha256 = sha256_serialized(&packet.semantic)?;
    if !rejects(&packet) {
        return Err("re-sealed direction-witness tamper was accepted".to_string());
    }
    let mut packet = project_one(subject, case, host, "m", "s")?;
    packet.semantic.producer_source_head = "stale".to_string();
    packet.semantic.producer_cargo_lock_sha256 = "stale".to_string();
    packet.semantic.profile = "release".to_string();
    packet.semantic.argv = vec!["stale".to_string()];
    packet.semantic.diff_sha256 = "stale".to_string();
    packet.semantic.subject_inputs.clear();
    packet.semantic.observed.finding_id = "stale".to_string();
    packet.host_evidence.current_ref = "stale/current.json".to_string();
    packet.host_evidence.stdout_sha256 = "stale".to_string();
    packet.semantic_sha256 = sha256_serialized(&packet.semantic)?;
    if !rejects(&packet) {
        return Err("coordinated authority re-seal tamper was accepted".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_strict_readback_rejects_duplicate_and_unknown_keys()
-> Result<(), String> {
    let fixture = fixture("should_gap")?;
    let packet = project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    let body = String::from_utf8(pretty_json(&packet)?)
        .map_err(|error| format!("packet JSON was not UTF-8: {error}"))?;
    let duplicate = body.replacen(
        "\"kind\": \"rust_judged_panel_portable_packet\"",
        "\"kind\": \"rust_judged_panel_portable_packet\",\n  \"kind\": \"duplicate\"",
        1,
    );
    let unknown = body.replacen('{', "{\n  \"unexpected\": true,", 1);
    for hostile in [duplicate, unknown] {
        if read_strict_json_bytes::<PortablePacket>(hostile.as_bytes(), "hostile packet").is_ok() {
            return Err("hostile packet passed strict readback".to_string());
        }
    }
    Ok(())
}

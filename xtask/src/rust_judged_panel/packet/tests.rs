use super::*;
use crate::rust_judged_panel::host_run::ValidatedInputDigest;

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

#[cfg(unix)]
fn directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

fn create_directory_symlink_or_skip(target: &Path, link: &Path) -> Result<bool, String> {
    match directory_symlink(target, link) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping portable symlink discriminator: {error}");
            Ok(false)
        }
        Err(error) => Err(format!(
            "create directory symlink `{}` -> `{}`: {error}",
            link.display(),
            target.display()
        )),
    }
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
        sha256: format!("sha256:{}", "a".repeat(64)),
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
            vec!["No static test path reaches the changed owner"],
            "Inspect the unresolved macro expansion.",
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
            "file": format!("target/ripr/rust-judged-panel/.staging-run-a/subjects/{case_id}/src/lib.rs"),
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
            "root": format!("target/ripr/rust-judged-panel/.staging-run-a/subjects/{case_id}"),
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
        executed_diff_identity: format!("sha256:{}", "b".repeat(64)),
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
        analyzer_input_identity: format!("sha256:{}", "b".repeat(64)),
        receipt_ref: format!(
            "target/ripr/rust-judged-panel/runs/run-a/cases/{case_id}/receipt.json"
        ),
        receipt_sha256: format!("sha256:{}", "c".repeat(64)),
        stdout_ref: format!("target/ripr/rust-judged-panel/runs/run-a/cases/{case_id}/stdout.bin"),
        stdout_sha256: format!("sha256:{}", "d".repeat(64)),
        stderr_ref: format!("target/ripr/rust-judged-panel/runs/run-a/cases/{case_id}/stderr.bin"),
        stderr_sha256: format!("sha256:{}", "e".repeat(64)),
        stdout: serde_json::to_vec(&report)
            .map_err(|error| format!("serialize fixture report: {error}"))?,
        materialized_root,
    };
    let host = ValidatedHostRun {
        current_ref: "target/ripr/rust-judged-panel/current.json".to_string(),
        current_sha256: format!("sha256:{}", "1".repeat(64)),
        index_ref: "target/ripr/rust-judged-panel/runs/run-a/run-index.json".to_string(),
        index_sha256: format!("sha256:{}", "2".repeat(64)),
        run_id: "run-a".to_string(),
        source_head: "3".repeat(40),
        source_tree: "4".repeat(40),
        cargo_lock_sha256: format!("sha256:{}", "5".repeat(64)),
        cargo_toml_sha256: format!("sha256:{}", "6".repeat(64)),
        profile: "dev".to_string(),
        features: vec!["default".to_string()],
        host_target: "test-target".to_string(),
        binary_sha256: format!("sha256:{}", "7".repeat(64)),
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
    for direction in ["should_gap", "should_stay_quiet", "should_limit"] {
        let fixture = fixture(direction)?;
        let packet = project_one(
            &fixture.subject,
            &fixture.case,
            &fixture.host,
            "sha256:manifest",
            "sha256:subjects",
        )?;
        require_eq(
            &packet.semantic.observed.expected_actionability,
            &fixture.subject.expected_actionability,
            "projected actionability authority",
        )?;
    }
    Ok(())
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
    let mut relocated_case = fixture.case.clone();
    let mut report = report_value(&relocated_case)?;
    let relocated_root = format!(
        "target/ripr/elsewhere/.staging-run-b/subjects/{}",
        fixture.subject.case_id
    );
    report["root"] = Value::String(relocated_root.clone());
    report["findings"][0]["probe"]["file"] = Value::String(format!("{relocated_root}/src/lib.rs"));
    store_report(&mut relocated_case, &report)?;
    let second = project_one(&fixture.subject, &relocated_case, &relocated, "m", "s")?;
    require_eq(
        &first.semantic_sha256,
        &second.semantic_sha256,
        "relocated semantic identity",
    )?;
    if first.host_evidence.run_id == second.host_evidence.run_id
        || first.host_evidence.binary_sha256 == second.host_evidence.binary_sha256
    {
        return Err("host provenance did not remain distinct".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_self_digest_and_direction_tamper() -> Result<(), String> {
    let fixture = fixture("should_gap")?;
    let mut packet = project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    let attestation = HostAttestation {
        case_id: packet.case_id.clone(),
        semantic: packet.semantic.clone(),
        host_evidence: packet.host_evidence.clone(),
    };
    let entry = PortableIndexEntry {
        case_id: packet.case_id.clone(),
        packet_path: format!("{PORTABLE_ROOT}/generation/packet.json"),
        packet_sha256: "sha256:file".to_string(),
        semantic_sha256: packet.semantic_sha256.clone(),
    };
    packet.semantic.observed.recommendation.clear();
    if validate_retained_packet(&packet, &entry, &fixture.subject, &attestation, "m", "s").is_ok() {
        return Err("semantic self-digest tamper was accepted".to_string());
    }
    packet.semantic_sha256 = sha256_serialized(&packet.semantic)?;
    let mut entry = entry;
    entry.semantic_sha256 = packet.semantic_sha256.clone();
    if validate_retained_packet(&packet, &entry, &fixture.subject, &attestation, "m", "s").is_ok() {
        return Err("re-sealed direction-witness tamper was accepted".to_string());
    }
    let mut stale = project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    stale.semantic.producer_source_head = "9".repeat(40);
    stale.semantic.profile = "release".to_string();
    stale.semantic.argv[4] = "stale-base".to_string();
    stale.host_evidence.binary_sha256 = format!("sha256:{}", "8".repeat(64));
    stale.host_evidence.receipt_ref = format!(
        "target/ripr/rust-judged-panel/runs/run-a/cases/{}/wrong.json",
        stale.case_id
    );
    stale.semantic_sha256 = sha256_serialized(&stale.semantic)?;
    let stale_entry = PortableIndexEntry {
        case_id: stale.case_id.clone(),
        packet_path: entry.packet_path.clone(),
        packet_sha256: sha256_bytes(&pretty_json(&stale)?),
        semantic_sha256: stale.semantic_sha256.clone(),
    };
    if validate_retained_packet(
        &stale,
        &stale_entry,
        &fixture.subject,
        &attestation,
        "m",
        "s",
    )
    .is_ok()
    {
        return Err("coordinated stale producer/invocation/host re-seal was accepted".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_invalid_complete_set_preserves_current() -> Result<(), String> {
    let root = test_root("invalid-complete")?;
    let current = root.0.join(CURRENT_PATH);
    fs::create_dir_all(
        current
            .parent()
            .ok_or_else(|| "current parent missing".to_string())?,
    )
    .map_err(|error| format!("create current parent: {error}"))?;
    fs::write(&current, b"old-current\n").map_err(|error| format!("seed current: {error}"))?;
    let fixtures = [
        fixture("should_gap")?,
        fixture("should_stay_quiet")?,
        fixture("should_limit")?,
    ];
    let subjects = fixtures
        .iter()
        .map(|fixture| fixture.subject.clone())
        .collect::<Vec<_>>();
    let mut packets = fixtures
        .iter()
        .map(|fixture| project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s"))
        .collect::<Result<Vec<_>, _>>()?;
    packets.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    let attestations = attestations_from_live_projection(&packets);
    let stale = packets
        .first_mut()
        .ok_or_else(|| "packet fixture missing".to_string())?;
    stale.semantic.profile = "release".to_string();
    stale.semantic.observed.recommendation = "Unrelated advice.".to_string();
    stale.semantic_sha256 = sha256_serialized(&stale.semantic)?;
    if publish_all(&root.0, "m", "s", &subjects, &attestations, &packets, None).is_ok() {
        return Err("invalid complete packet set was published".to_string());
    }
    let retained = fs::read(&current).map_err(|error| format!("read retained current: {error}"))?;
    if retained != b"old-current\n" {
        return Err("invalid complete set advanced authoritative current".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_rejects_nested_symlink_escape() -> Result<(), String> {
    let root = test_root("portable-link")?;
    let outside = test_root("portable-link-outside")?;
    fs::write(outside.0.join("packet.json"), b"{}\n")
        .map_err(|error| format!("write outside packet: {error}"))?;
    let portable = root.0.join("metrics/rust-judged-behavior-panel/portable");
    fs::create_dir_all(&portable).map_err(|error| format!("create portable root: {error}"))?;
    let link = portable.join("generations");
    if !create_directory_symlink_or_skip(&outside.0, &link)? {
        return Ok(());
    }
    let escaped = confined_existing_file(
        &root.0,
        Path::new("metrics/rust-judged-behavior-panel/portable/generations/packet.json"),
        "portable packet",
    )
    .is_err();
    fs::remove_dir(&link).map_err(|error| format!("remove portable link: {error}"))?;
    if escaped {
        Ok(())
    } else {
        Err("portable nested symlink escape was accepted".to_string())
    }
}

#[test]
fn rust_judged_panel_packet_partial_and_concurrent_publication_fail_closed() -> Result<(), String> {
    let root = test_root("publication")?;
    let current = root.0.join(CURRENT_PATH);
    fs::create_dir_all(
        current
            .parent()
            .ok_or_else(|| "current parent missing".to_string())?,
    )
    .map_err(|error| format!("create current parent: {error}"))?;
    fs::write(&current, b"old-current\n").map_err(|error| format!("seed current: {error}"))?;
    let fixture = fixture("should_gap")?;
    let base = project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    let mut packets = Vec::new();
    let mut subjects = Vec::new();
    for suffix in ["a", "b", "c"] {
        let mut packet = base.clone();
        packet.case_id = format!("case-{suffix}");
        let mut subject = fixture.subject.clone();
        subject.case_id = packet.case_id.clone();
        subjects.push(subject);
        packets.push(packet);
    }
    let attestations = attestations_from_live_projection(&packets);
    if publish_all(
        &root.0,
        "m",
        "s",
        &subjects,
        &attestations,
        &packets,
        Some(2),
    )
    .is_ok()
    {
        return Err("injected partial publication succeeded".to_string());
    }
    let retained = fs::read(&current).map_err(|error| format!("read retained current: {error}"))?;
    if retained != b"old-current\n" {
        return Err("partial publication changed the authoritative current".to_string());
    }
    let staging = root.0.join(STAGING_ROOT);
    fs::create_dir_all(&staging).map_err(|error| format!("create staging: {error}"))?;
    fs::write(staging.join("packet.lock"), b"held\n")
        .map_err(|error| format!("hold packet lock: {error}"))?;
    if publish_all(&root.0, "m", "s", &subjects, &attestations, &packets, None).is_ok() {
        return Err("concurrent packet publisher was accepted".to_string());
    }
    Ok(())
}

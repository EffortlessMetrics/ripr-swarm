//! Admission of the existing #11919/#12404 installed ordinary-loop producer.
//! The manifest pins bytes; it cannot assert execution, movement, or clocks.
use super::{FirstHourRepairFunnelReportV1, FunnelCaseRowV1, INPUTS_SCHEMA_VERSION, project_case};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};
const MANIFEST_SCHEMA: &str = "retained_installed_ordinary_manifest.v1";
const SELECTOR: &str =
    "ordinary_daily_loop::ordinary_loop_preserves_production_subject_and_native_outcome";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    cohort: String,
    bundles: Vec<Bundle>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    directory: String,
    files: BTreeMap<String, String>,
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("retained receipt needs nonempty {key}"))
}

fn hex_text<'a>(value: &'a Value, key: &str, width: usize) -> Result<&'a str, String> {
    let value = text(value, key)?;
    require(
        value.len() == width
            && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "malformed source or digest identity",
    )?;
    Ok(value)
}

fn object<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value
        .get(key)
        .filter(|v| v.is_object())
        .ok_or_else(|| format!("missing identity object {key}"))
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn relative(path: &str) -> bool {
    !path.is_empty()
            && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
struct Receipts {
    bytes: BTreeMap<String, Vec<u8>>,
    hashes: BTreeMap<String, String>,
}
impl Receipts {
    fn load(parent: &Path, bundle: &Bundle) -> Result<Self, String> {
        require(
            relative(&bundle.directory),
            "bundle directory must be relative without traversal",
        )?;
        let parent = parent.canonicalize().map_err(|e| e.to_string())?;
        let root = parent.join(&bundle.directory).canonicalize().map_err(|e| e.to_string())?;
        require(root.starts_with(&parent), "bundle directory escapes manifest parent")?;
        require(bundle.files.len() <= 1024, "too many receipt files")?;
        let mut bytes = BTreeMap::new();
        let mut total = 0_u64;
        for (name, expected) in &bundle.files {
            require(relative(name), "receipt path must be relative without traversal")?;
            let path = root.join(name).canonicalize().map_err(|e| format!("{name}: {e}"))?;
            require(path.starts_with(&root), "receipt symlink escapes bundle")?;
            let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            total = total.checked_add(size).ok_or("receipt size overflow")?;
            require(total <= 64 * 1024 * 1024, "receipt bundle exceeds 64 MiB")?;
            let body = fs::read(path).map_err(|e| format!("{name}: {e}"))?;
            require(digest(&body) == *expected, &format!("receipt digest mismatch: {name}"))?;
            bytes.insert(name.clone(), body);
        }
        Ok(Self {
            bytes,
            hashes: bundle.files.clone(),
        })
    }

    fn json(&self, name: &str) -> Result<Value, String> {
        let bytes = self.bytes.get(name).ok_or_else(|| format!("missing retained receipt {name}"))?;
        serde_json::from_slice(bytes).map_err(|e| format!("{name}: {e}"))
    }

    fn commands(&self) -> Result<Vec<(Value, Value, Vec<u8>)>, String> {
        let mut commands = Vec::new();
        for name in self.bytes.keys().filter(|n| n.ends_with("/command.json")) {
            let prefix = name.strip_suffix("command.json").ok_or("command path")?;
            let command = self.json(name)?;
            let result = self.json(&format!("{prefix}result.json"))?;
            for stream in ["stdout", "stderr"] {
                let bytes = self.bytes.get(&format!("{prefix}{stream}"))
                    .ok_or("missing raw command stream")?;
                require(
                    result
                        .get(format!("{stream}_sha256"))
                        .and_then(Value::as_str)
                        == Some(digest(bytes).as_str()),
                    "raw command stream digest mismatch",
                )?;
            }
            commands.push((command, result, self.bytes[&format!("{prefix}stdout")].clone()));
        }
        require(!commands.is_empty(), "no retained command evidence")?;
        Ok(commands)
    }
}

fn clean(result: &Value) -> bool {
    ["capture_error", "cleanup_error", "harness_stop", "wait_error"]
        .iter()
        .all(|key| result.get(key).is_some_and(Value::is_null))
}

fn one_finding(report: &Value) -> Result<&Value, String> {
    require(
        report.get("schema_version").and_then(Value::as_str) == Some("0.2"),
        "unsupported check schema",
    )?;
    require(
        report.pointer("/analysis_outcome/analysis_complete") == Some(&json!(true)),
        "incomplete retained analysis",
    )?;
    let findings = report.get("findings").and_then(Value::as_array).ok_or("missing findings")?;
    require(findings.len() == 1, "ordinary producer requires exactly one finding")?;
    let finding = findings.first().ok_or("missing finding")?;
    let gap = object(finding, "canonical_gap")?;
    let probe = object(finding, "probe")?;
    require(
        text(finding, "canonical_gap_id")? == text(gap, "id")?,
        "canonical gap ID/content mismatch",
    )?;
    for field in [
        "language",
        "file",
        "owner",
        "behavior_kind",
        "normalized_discriminator",
        "probe_kind",
    ] {
        text(gap, field)?;
    }
    for field in ["id", "file", "family", "expression"] {
        text(probe, field)?;
    }
    require(text(finding, "source_currentness")? == "candidate_current", "finding is not current")?;
    text(finding, "classification")?;
    Ok(finding)
}

fn bind_check(
    report: &Value,
    subject: &Value,
    root: &str,
    commands: &[(Value, Value, Vec<u8>)],
    binary: &str,
) -> Result<(), String> {
    one_finding(report)?;
    require(
        text(report, "root")? == root
            && report.get("base_commit") == subject.get("base")
            && report.pointer("/head/commit") == subject.get("head")
            && report.pointer("/head/source") == Some(&json!("working_tree")),
        "check root/base/head mismatch",
    )?;
    let base = text(subject, "base")?;
    let expected = json!([
        "check", "--root", root, "--base", base, "--worktree", "--format", "json"
    ]);
    let matched = commands.iter().any(|(command, result, stdout)| {
        command.get("program").and_then(Value::as_str) == Some(binary)
            && command.get("args") == Some(&expected)
            && clean(result)
            && result.get("native_exit_code") == Some(&json!(0))
            && serde_json::from_slice::<Value>(stdout).ok().as_ref() == Some(report)
    });
    require(matched, "check has no matching successful raw command")
}

fn native_facts(
    native: &Value,
    root: &str,
    commands: &[(Value, Value, Vec<u8>)],
) -> Result<(bool, bool), String> {
    let selector = text(native, "selector")?;
    require(selector == "equality_boundary_discounts", "unsupported native selector")?;
    let argv = native
        .get("command")
        .and_then(Value::as_array)
        .ok_or("missing native command")?;
    require(
        native.get("command")
            == Some(&json!([
                "cargo",
                "test",
                "--manifest-path",
                format!("{root}/Cargo.toml"),
                "--test",
                "pricing",
                selector,
                "--",
                "--exact",
                "--nocapture"
            ])),
        "native command subject mismatch",
    )?;
    let selected = native
        .get("selected")
        .and_then(Value::as_u64)
        .ok_or("missing selected count")?;
    let executed = native
        .get("executed")
        .and_then(Value::as_u64)
        .ok_or("missing executed count")?;
    let passed = native
        .get("passed")
        .and_then(Value::as_u64)
        .ok_or("missing passed count")?;
    let failed = native
        .get("failed")
        .and_then(Value::as_u64)
        .ok_or("missing failed count")?;
    require(
        selected <= 1
            && passed.checked_add(failed) == Some(executed)
            && executed == selected
            && native.get("ignored") == Some(&json!(0)),
        "inconsistent native denominator",
    )?;
    let exit = native
        .get("native_exit_code")
        .and_then(Value::as_i64)
        .ok_or("missing native exit status")?;
    require(
        (exit == 0 && failed == 0) || (exit != 0 && failed > 0),
        "native exit and failure count disagree",
    )?;
    let matched = commands.iter().any(|(command, result, stdout)| {
        let output = String::from_utf8_lossy(stdout);
        let summary = format!("{passed} passed; {failed} failed; 0 ignored;");
        command.get("program") == argv.first()
            && command.get("args") == Some(&json!(&argv[1..]))
            && clean(result)
            && result.get("native_exit_code") == native.get("native_exit_code")
            && output.lines().filter(|l| l.starts_with("test result: ")).count() == 1
            && output.lines().any(|l| l.starts_with("test result: ") && l.contains(&summary))
            && output.lines().any(|l| {
                l == format!(
                    "running {selected} {}",
                    if selected == 1 { "test" } else { "tests" }
                )
            })
            && (executed == 0
                || output.lines().any(|l| {
                    l == format!(
                        "test {selector} ... {}",
                        if failed == 0 { "ok" } else { "FAILED" }
                    )
                }))
    });
    require(matched, "native fact has no matching raw execution")?;
    Ok((
        executed > 0,
        executed == 1
            && passed == 1
            && failed == 0
            && native.get("native_exit_code") == Some(&json!(0)),
    ))
}

fn project(receipts: &Receipts) -> Result<FunnelCaseRowV1, String> {
    let admitted = receipts.json("journey/admitted.json")?;
    let invocation = receipts.json("invocation.json")?;
    let wheel = receipts.json("wheel-receipt.json")?;
    let runner = receipts.json("runner-receipt.json")?;
    let subject = receipts.json("journey/subject-before.json")?;
    let before = receipts.json("journey/before.json")?;
    let commands = receipts.commands()?;
    require(wheel.get("schema_version") == Some(&json!(2)), "unsupported wheel receipt schema")?;
    require(
        invocation.get("mode") == Some(&json!("installed"))
            && text(&invocation, "selector")? == SELECTOR
            && text(&runner, "test_selector")? == SELECTOR,
        "not the strict installed ordinary producer",
    )?;
    let candidate = hex_text(&invocation, "candidate_sha", 40)?;
    let binary = text(&admitted, "resolved_binary")?;
    require(
        hex_text(&wheel, "candidate_sha", 40)? == candidate
            && hex_text(&runner, "candidate_sha", 40)? == candidate
            && hex_text(&wheel, "candidate_tree", 40)? == hex_text(&runner, "candidate_tree", 40)?
            && hex_text(&runner, "runner_sha256", 64)?
                == hex_text(&invocation, "runner_sha256", 64)?,
        "candidate/runner source mismatch",
    )?;
    let payload = hex_text(&invocation, "payload_sha256", 64)?;
    for field in ["base", "base_tree", "head", "head_tree"] {
        hex_text(&subject, field, 40)?;
    }
    require(
        text(&admitted, "sha256")? == format!("sha256:{payload}")
            && wheel.pointer("/wheel/installed_payload_sha256") == invocation.get("payload_sha256")
            && admitted.get("full_version") == invocation.get("full_version")
            && text(&admitted, "full_version")?.ends_with(&format!("({candidate})"))
            && invocation.get("installed_binary") == admitted.get("resolved_binary"),
        "installed binary identity mismatch",
    )?;
    for (name, field) in [
        ("journey/production.rs", "production_sha256"),
        ("journey/Cargo.toml", "manifest_sha256"),
        ("journey/Cargo.lock", "lockfile_sha256"),
    ] {
        require(
            receipts.hashes.get(name).map(String::as_str) == Some(text(&subject, field)?),
            "subject source digest mismatch",
        )?;
    }
    require(
        commands.iter().any(|(command, result, stdout)| {
            command.get("program").and_then(Value::as_str) == Some(binary)
                && command.get("args") == Some(&json!(["--version"]))
                && clean(result)
                && result.get("native_exit_code") == Some(&json!(0))
                && String::from_utf8_lossy(stdout).trim_end()
                    == text(&admitted, "full_version").unwrap_or("")
        }),
        "installed version has no matching raw command",
    )?;
    let root = text(&before, "root")?;
    bind_check(&before, &subject, root, &commands, binary)?;
    let finding = one_finding(&before)?;
    let gap = text(finding, "canonical_gap_id")?;
    let mut movement = "unknown";
    let mut after_present = false;
    if receipts.bytes.contains_key("journey/after.json") {
        let after = receipts.json("journey/after.json")?;
        require(
            receipts.json("journey/subject-after.json")? == subject,
            "production subject changed",
        )?;
        bind_check(&after, &subject, root, &commands, binary)?;
        let next = one_finding(&after)?;
        require(
            text(next, "canonical_gap_id")? == gap
                && object(next, "canonical_gap")? == object(finding, "canonical_gap")?
            && object(next, "probe")? == object(finding, "probe")?,
            "after item/source mismatch",
        )?;
        movement = if finding.get("classification") == Some(&json!("weakly_exposed"))
            && next.get("classification") == Some(&json!("exposed")) {
            "improved"
        } else {
            "unchanged"
        };
        after_present = true;
    }
    let (ran, passed) = if receipts.bytes.contains_key("journey/native-after.json") {
        native_facts(&receipts.json("journey/native-after.json")?, root, &commands)?
    } else {
        (false, false)
    };
    let case_id = digest(
        format!(
            "{candidate}\n{root}\n{}\n{}\n{gap}",
            text(&subject, "base")?,
            text(&subject, "head")?
        )
        .as_bytes(),
    );
    let mut row = project_case(&json!({
        "schema_version": INPUTS_SCHEMA_VERSION,
        "case_id": case_id,
        "cohort": "installed_deterministic_journey",
        "clocks_compatible": false,
        "after_analysis_complete": after_present,
        "repair_attempt": {
            "verify_command": "cargo test --test pricing equality_boundary_discounts -- --exact --nocapture",
            "verify_executed": ran,
            "verify_result": if passed { "pass" } else if ran { "failed" } else { "not_run" },
            "static_movement": movement
        }
    }))?;
    row.stages.insert(
        0,
        super::StageObservationV1 {
            stage: "first_check_complete".into(),
            at_ms: None,
            receipt_digest: receipts.hashes["journey/before.json"].clone(),
            not_measured_reason: Some("legacy producer retains no shared stage clock".into()),
        },
    );
    row.stages.insert(
        1,
        super::StageObservationV1 {
            stage: "first_honest_result".into(),
            at_ms: None,
            receipt_digest: receipts.hashes["journey/before.json"].clone(),
            not_measured_reason: Some(
                "controlled static result; no install or shared stage clock".into(),
            ),
        },
    );
    // These are ordinary observations, never a reconstructed RepairAttempt.
    for stage in &mut row.stages {
        stage.receipt_digest = match stage.stage.as_str() {
            "project_verification_executed" => receipts.hashes.get("journey/native-after.json"),
            "after_analysis_complete" => receipts.hashes.get("journey/after.json"),
            "verified_improvement_or_closure" => receipts.hashes.get("journey/after.json"),
            "first_check_complete" | "first_honest_result" => {
                receipts.hashes.get("journey/before.json")
            }
            _ => None,
        }
        .cloned()
        .ok_or("stage without admitted receipt")?;
    }
    row.identity.insert("candidate_sha".into(), candidate.into());
    row.identity.insert("binary_sha256".into(), text(&admitted, "sha256")?.into());
    row.identity.insert("root".into(), root.into());
    row.identity.insert("base".into(), text(&subject, "base")?.into());
    row.identity.insert("head".into(), text(&subject, "head")?.into());
    row.identity.insert("item".into(), gap.into());
    row.identity.insert(
        "producer".into(),
        "#11919/#12404 strict installed ordinary-loop (legacy receipt shape; not verifier attestation)".into(),
    );
    row.identity.insert(
        "non_claims".into(),
        "installed deterministic fixture; no real repair, personal adoption, install clock, RepairCard or terminal RepairAttempt receipt".into(),
    );
    let missing: Vec<&str> = super::STAGES
        .iter()
        .copied()
        .filter(|name| !row.stages.iter().any(|s| s.stage == *name))
        .collect();
    row.identity.insert("missing_stages".into(), missing.join(", "));
    for (name, hash) in &receipts.hashes {
        row.identity.insert(format!("source:{name}"), hash.clone());
    }
    Ok(row)
}

pub(super) fn report(path: &Path) -> Result<FirstHourRepairFunnelReportV1, String> {
    let bytes = fs::read(path).map_err(|e| format!("read input manifest: {e}"))?;
    report_bytes(path, &bytes)
}

fn report_bytes(path: &Path, bytes: &[u8]) -> Result<FirstHourRepairFunnelReportV1, String> {
    require(bytes.len() <= 1024 * 1024, "input manifest exceeds 1 MiB")?;
    let manifest: Manifest = serde_json::from_slice(bytes)
        .map_err(|e| format!("input manifest: {e}"))?;
    require(manifest.schema_version == MANIFEST_SCHEMA, "unsupported retained manifest schema")?;
    require(
        manifest.cohort == "installed_deterministic_journey",
        "installed fixture cannot be claimed as a real cohort",
    )?;
    require(!manifest.bundles.is_empty(), "manifest contains no bundles")?;
    let mut cases = Vec::new();
    let mut seen = BTreeMap::new();
    for bundle in &manifest.bundles {
        let mut row = project(&Receipts::load(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            bundle,
        )?)?;
        let normalized = serde_json::to_value(&row).map_err(|e| e.to_string())?;
        if let Some(previous) = seen.insert(row.case_id.clone(), normalized.clone()) {
            require(previous == normalized, "conflicting duplicate opportunity views")?;
            continue;
        }
        row.identity.insert("manifest_sha256".into(), digest(bytes));
        cases.push(row);
    }
    Ok(FirstHourRepairFunnelReportV1 {
        schema_version: super::REPORT_SCHEMA_VERSION.into(),
        claim_boundary: super::CLAIM_BOUNDARY.into(),
        corpus_path: path.display().to_string(),
        case_count: cases.len() as u64,
        selected_count: cases.len() as u64,
        control_failures: Vec::new(),
        cases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_path() -> std::path::PathBuf {
        super::super::workspace_path("fixtures/repair_funnel/installed-41383028929.json")
    }

    fn retained() -> Result<Receipts, String> {
        let path = manifest_path();
        let manifest: Manifest = serde_json::from_slice(&fs::read(&path)
            .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Receipts::load(path.parent()
            .ok_or("manifest parent")?, manifest.bundles.first().ok_or("bundle")?)
    }

    fn repin(receipts: &mut Receipts, name: &str, value: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        receipts.hashes.insert(name.into(), digest(&bytes));
        receipts.bytes.insert(name.into(), bytes);
        Ok(())
    }

    fn repin_check(
        receipts: &mut Receipts,
        name: &str,
        command: usize,
        value: &Value,
    ) -> Result<(), String> {
        repin(receipts, name, value)?;
        let stdout = format!("journey/command-{command:03}/stdout");
        let result = format!("journey/command-{command:03}/result.json");
        let bytes = receipts.bytes.get(name).ok_or("repinned check")?.clone();
        let mut status = receipts.json(&result)?;
        status["stdout_sha256"] = json!(digest(&bytes));
        receipts.hashes.insert(stdout.clone(), digest(&bytes));
        receipts.bytes.insert(stdout, bytes);
        repin(receipts, &result, &status)
    }

    #[test]
    fn repair_funnel_required_identity_fields_cannot_disappear_in_pairs() -> Result<(), String> {
        for (left, right, field) in [
            ("wheel-receipt.json", "runner-receipt.json", "candidate_tree"),
            ("runner-receipt.json", "invocation.json", "runner_sha256"),
        ] {
            let mut receipts = retained()?;
            for name in [left, right] {
                let mut value = receipts.json(name)?;
                value.as_object_mut().ok_or("object")?.remove(field);
                repin(&mut receipts, name, &value)?;
            }
            assert!(project(&receipts).is_err(), "missing paired {field}");
        }
        for field in ["canonical_gap", "probe"] {
            let mut receipts = retained()?;
            for (name, command) in [("journey/before.json", 1), ("journey/after.json", 11)] {
                let mut value = receipts.json(name)?;
                value["findings"][0].as_object_mut().ok_or("finding")?.remove(field);
                repin_check(&mut receipts, name, command, &value)?;
            }
            assert!(project(&receipts).is_err(), "missing paired {field}");
        }
        let mut receipts = retained()?;
        let mut after = receipts.json("journey/after.json")?;
        after["findings"][0]["canonical_gap_id"] = json!("unrelated-gap");
        after["findings"][0]["canonical_gap"]["id"] = json!("unrelated-gap");
        repin_check(&mut receipts, "journey/after.json", 11, &after)?;
        assert!(
            project(&receipts).is_err(),
            "another internally consistent gap cannot borrow movement"
        );
        Ok(())
    }

    #[test]
    fn repair_funnel_native_pass_without_static_movement_stays_limited() -> Result<(), String> {
        let mut receipts = retained()?;
        let mut after = receipts.json("journey/after.json")?;
        after["findings"][0]["classification"] = json!("weakly_exposed");
        repin_check(&mut receipts, "journey/after.json", 11, &after)?;
        let row = project(&receipts)?;
        assert_eq!(row.funnel_exit, "honest_static_limitation");
        assert!(!row.stages.iter().any(|s| s.stage == "verified_improvement_or_closure"));
        Ok(())
    }

    #[test]
    fn repair_funnel_retained_installed_public_input_round_trip() -> Result<(), String> {
        let report = report(&manifest_path())?;
        assert_eq!(report.case_count, 1);
        let row = report.cases.first().ok_or("row")?;
        assert_eq!(row.cohort, "installed_deterministic_journey");
        assert_eq!(row.funnel_exit, "verified_improvement");
        assert!(row.control_violations.is_empty());
        assert!(row.stages.iter().any(|s| s.stage == "first_honest_result"));
        assert!(row.stages.iter().all(|s| s.at_ms.is_none()));
        assert!(!row.stages.iter().any(|s| {
            s.stage == "repair_started"
                || s.stage == "install_complete"
                || s.stage == "terminal_receipt_issued"
        }));
        assert!(row.metrics.iter().all(|m| m.state == "not_measured" && m.sample_count == 0));
        let value = serde_json::to_value(&report).map_err(|e| e.to_string())?;
        let input = super::super::repair_funnel_to_input(&value)?;
        assert!(input["metrics"].as_array()
            .ok_or("metrics")?.iter().all(|m| m.get("completed") == Some(&json!(false))));
        let markdown = super::super::repair_funnel_markdown(&report);
        assert!(markdown.contains("installed_deterministic_journey"));
        assert!(markdown.contains("no real repair, personal adoption"));
        Ok(())
    }

    #[test]
    fn repair_funnel_rejects_manifest_digest_and_real_cohort_tampering() -> Result<(), String> {
        let path = manifest_path();
        let mut value: Value = serde_json::from_slice(&fs::read(&path)
            .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        value["cohort"] = json!("governed_real_repository_attempt");
        assert!(report_bytes(&path, &serde_json::to_vec(&value)
            .map_err(|e| e.to_string())?).is_err());
        value["cohort"] = json!("installed_deterministic_journey");
        value["bundles"][0]["files"]["journey/before.json"] = json!("sha256:wrong");
        assert!(report_bytes(&path, &serde_json::to_vec(&value)
            .map_err(|e| e.to_string())?).is_err());
        Ok(())
    }

    #[test]
    fn repair_funnel_rejects_repinned_identity_and_raw_stream_tampering() -> Result<(), String> {
        for file in ["invocation.json", "journey/subject-before.json", "journey/after.json"] {
            let mut receipts = retained()?;
            let mut value = receipts.json(file)?;
            match file {
                "invocation.json" => value["payload_sha256"] = json!("wrong"),
                "journey/subject-before.json" => value["base"] = json!("wrong"),
                _ => value["root"] = json!("wrong-root"),
            }
            let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
            receipts.hashes.insert(file.into(), digest(&bytes));
            receipts.bytes.insert(file.into(), bytes);
            assert!(project(&receipts).is_err(), "{file}");
        }
        let mut receipts = retained()?;
        receipts.bytes.insert(
            "journey/command-010/stdout".into(),
            b"test result: ok. 1 passed".to_vec(),
        );
        assert!(project(&receipts).is_err());
        Ok(())
    }

    #[test]
    fn repair_funnel_retains_zero_failed_and_missing_native_without_credit() -> Result<(), String> {
        for (file, exit) in [
            ("journey/native-zero-selection.json", "verification_not_run"),
            ("journey/native-wrong-expected.json", "verification_failed"),
        ] {
            let mut receipts = retained()?;
            let bytes = receipts.bytes.get(file).ok_or("native control")?.clone();
            receipts.hashes.insert("journey/native-after.json".into(), digest(&bytes));
            receipts.bytes.insert("journey/native-after.json".into(), bytes);
            let row = project(&receipts)?;
            assert_eq!(row.funnel_exit, exit);
            assert!(!row.stages.iter().any(|s| s.stage == "verified_improvement_or_closure"));
        }
        let mut receipts = retained()?;
        receipts.bytes.remove("journey/native-after.json");
        receipts.hashes.remove("journey/native-after.json");
        let row = project(&receipts)?;
        assert_eq!(row.funnel_exit, "verification_not_run");
        let mut receipts = retained()?;
        receipts.bytes.remove("journey/after.json");
        receipts.hashes.remove("journey/after.json");
        let row = project(&receipts)?;
        assert!(!row.stages.iter().any(|s| {
            s.stage == "after_analysis_complete" || s.stage == "verified_improvement_or_closure"
        }));
        Ok(())
    }

    #[test]
    fn repair_funnel_refuses_repinned_native_and_check_command_argv() -> Result<(), String> {
        for wrong_subcommand in [true, false] {
            let mut receipts = retained()?;
            let mut native = receipts.json("journey/native-after.json")?;
            if wrong_subcommand {
                native["command"][1] = json!("run");
            } else {
                native["command"]
                    .as_array_mut()
                    .ok_or("native argv")?
                    .push(json!("--release"));
            }
            let mut raw_command = receipts.json("journey/command-010/command.json")?;
            let argv = native["command"].as_array().ok_or("native argv")?;
            raw_command["args"] = json!(&argv[1..]);
            repin(&mut receipts, "journey/native-after.json", &native)?;
            repin(&mut receipts, "journey/command-010/command.json", &raw_command)?;
            assert!(project(&receipts).is_err(), "repinned unsupported Cargo argv");
        }
        let mut receipts = retained()?;
        let mut raw_command = receipts.json("journey/command-001/command.json")?;
        raw_command["args"]
            .as_array_mut()
            .ok_or("check argv")?
            .push(json!("--quiet"));
        repin(&mut receipts, "journey/command-001/command.json", &raw_command)?;
        assert!(project(&receipts).is_err(), "repinned unsupported check argv");
        Ok(())
    }

    #[test]
    fn repair_funnel_conflicting_views_reject_missing_native() -> Result<(), String> {
        let path = manifest_path();
        let mut value: Value = serde_json::from_slice(
            &fs::read(&path).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut missing_native = value["bundles"][0].clone();
        missing_native["files"]
            .as_object_mut()
            .ok_or("files")?
            .remove("journey/native-after.json");
        value["bundles"]
            .as_array_mut()
            .ok_or("bundles")?
            .push(missing_native);
        let result = report_bytes(
            &path,
            &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
        );
        let Err(error) = result else {
            return Err("conflicting duplicate silently hid missing native execution".into());
        };
        assert_eq!(error, "conflicting duplicate opportunity views");
        Ok(())
    }

    #[test]
    fn repair_funnel_duplicate_channel_views_count_one_opportunity() -> Result<(), String> {
        let path = manifest_path();
        let mut value: Value = serde_json::from_slice(&fs::read(&path)
            .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let bundle = value["bundles"][0].clone();
        value["bundles"].as_array_mut().ok_or("bundles")?.push(bundle);
        let report = report_bytes(&path, &serde_json::to_vec(&value).map_err(|e| e.to_string())?)?;
        assert_eq!(report.case_count, 1);
        Ok(())
    }
}

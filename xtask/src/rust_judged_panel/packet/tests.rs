use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::rust_judged_panel::host_run::ValidatedInputDigest;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use super::{remove_replacement_backup, replacement_backup_path};

fn scratch(name: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("clock before epoch: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-packet-{name}-{nonce}"));
    fs::create_dir_all(&root).map_err(|error| format!("create scratch root: {error}"))?;
    Ok(root)
}

#[test]
fn rust_judged_panel_packet_replaces_existing_current_on_second_publication() -> Result<(), String>
{
    let root = scratch("replace")?;
    let current = root.join("current.json");
    let first = root.join("first.tmp");
    let second = root.join("second.tmp");
    fs::write(&first, b"generation-a\n").map_err(|error| error.to_string())?;
    replace_file(&first, &current)?;
    fs::write(&second, b"generation-b\n").map_err(|error| error.to_string())?;
    replace_file(&second, &current)?;
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    let cleanup = fs::remove_dir_all(&root);
    if cleanup.is_err() || actual != b"generation-b\n" || first.exists() || second.exists() {
        return Err("second publication did not replace the prior current atomically".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_failed_current_replacement_preserves_previous_pointer()
-> Result<(), String> {
    let root = scratch("failed-replace")?;
    let current = root.join("current.json");
    let missing = root.join("missing.tmp");
    fs::write(&current, b"prior-generation\n").map_err(|error| error.to_string())?;
    if replace_file(&missing, &current).is_ok() {
        return Err("missing replacement unexpectedly succeeded".to_string());
    }
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    if actual == b"prior-generation\n" {
        Ok(())
    } else {
        Err("failed replacement destroyed the previous current pointer".to_string())
    }
}

#[cfg(windows)]
#[test]
fn rust_judged_panel_packet_reconciles_stale_backup_before_publication() -> Result<(), String> {
    let root = scratch("stale-backup")?;
    let current = root.join("current.json");
    let backup = replacement_backup_path(&current);
    let next = root.join("next.tmp");
    fs::write(&current, b"current-generation\n").map_err(|error| error.to_string())?;
    fs::write(&backup, b"stale-generation\n").map_err(|error| error.to_string())?;
    fs::write(&next, b"next-generation\n").map_err(|error| error.to_string())?;
    replace_file(&next, &current)?;
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    let cleanup = fs::remove_dir_all(&root);
    if cleanup.is_err() || actual != b"next-generation\n" || backup.exists() {
        return Err("stale backup was not reconciled after publication".to_string());
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn rust_judged_panel_packet_recovers_interrupted_staging_before_publication() -> Result<(), String>
{
    let root = scratch("recover-backup")?;
    let current = root.join("current.json");
    let backup = replacement_backup_path(&current);
    let next = root.join("next.tmp");
    fs::write(&backup, b"prior-generation\n").map_err(|error| error.to_string())?;
    fs::write(&next, b"next-generation\n").map_err(|error| error.to_string())?;
    replace_file(&next, &current)?;
    let actual = fs::read(&current).map_err(|error| error.to_string())?;
    let cleanup = fs::remove_dir_all(&root);
    if cleanup.is_err() || actual != b"next-generation\n" || backup.exists() {
        return Err("interrupted staging was not recovered before publication".to_string());
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn rust_judged_panel_packet_reports_backup_cleanup_failure() -> Result<(), String> {
    let root = scratch("cleanup-failure")?;
    let current = root.join("current.json");
    let backup = replacement_backup_path(&current);
    fs::write(&current, b"prior-generation\n").map_err(|error| error.to_string())?;
    fs::create_dir(&backup).map_err(|error| error.to_string())?;
    let result = remove_replacement_backup(&current, &backup);
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    if result.is_ok() {
        return Err("backup cleanup failure was not reported after publication".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_strict_outer_current_rejects_duplicate_and_unknown_keys()
-> Result<(), String> {
    let valid = br#"{"schema_version":"0.1","kind":"rust_judged_panel_portable_current","generation_id":"generation","index_path":"metrics/rust-judged-behavior-panel/portable/generations/generation/packet-index.json","index_sha256":"sha256:index"}"#;
    let duplicate = br#"{"schema_version":"0.1","kind":"rust_judged_panel_portable_current","kind":"duplicate","generation_id":"generation","index_path":"metrics/rust-judged-behavior-panel/portable/generations/generation/packet-index.json","index_sha256":"sha256:index"}"#;
    let unknown = br#"{"schema_version":"0.1","kind":"rust_judged_panel_portable_current","generation_id":"generation","index_path":"metrics/rust-judged-behavior-panel/portable/generations/generation/packet-index.json","index_sha256":"sha256:index","unexpected":true}"#;
    let parsed: PortableCurrent = read_strict_json_bytes(valid, "valid current")?;
    if parsed.generation_id != "generation"
        || read_strict_json_bytes::<PortableCurrent>(duplicate, "duplicate current").is_ok()
        || read_strict_json_bytes::<PortableCurrent>(unknown, "unknown current").is_ok()
    {
        return Err("strict current readback accepted invalid JSON".to_string());
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_retained_validator_reaches_committed_generation() -> Result<(), String>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask has no workspace parent".to_string())?;
    let manifest = crate::rust_judged_panel::load_and_validate_at(
        root,
        Path::new("metrics/rust-judged-behavior-panel/manifest.json"),
    )?;
    super::validate_at(root, &manifest)
}

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    _root: TestRoot,
    subject: PacketSubject,
    case: ValidatedHostCase,
    host: ValidatedHostRun,
}

fn file(role: &str, repository_path: &str) -> ReplaySubjectFile {
    ReplaySubjectFile {
        source_path: format!("metrics/subjects/{role}"),
        repository_path: repository_path.to_string(),
        sha256: format!("sha256:{role}"),
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn fixture(direction: &str) -> Result<Fixture, String> {
    let (
        case_id,
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
            "discounted_total",
            "predicate_boundary",
            "predicate",
            "amount >= threshold",
            "weakly_exposed",
            "repair_candidate",
            None,
            vec!["Missing discriminator value: amount == threshold"],
            "Add boundary tests for below, equal, and above the changed threshold with exact assertions.",
        ),
        "should_stay_quiet" => (
            "rust-boundary-exact-equality-should-stay-quiet",
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
        other => return Err(format!("unsupported direction `{other}`")),
    };
    let cargo_toml = file("cargo_toml", "Cargo.toml");
    let cargo_lock = file("cargo_lock", "Cargo.lock");
    let config = file("config", "ripr.toml");
    let source_before = file("source_before", "src/lib.rs");
    let source_after = file("source_after", "src/lib.rs");
    let diff = file("diff", "selected.diff");
    let tests = vec![file("test", "tests/case.rs")];
    let subject = PacketSubject {
        case_id: case_id.to_string(),
        subject_id: case_id.to_string(),
        repository: "synthetic".to_string(),
        expected_direction: direction.to_string(),
        anchor_file: "src/lib.rs".to_string(),
        anchor_line: 2,
        owner: owner.to_string(),
        behavior_family: family.to_string(),
        changed_behavior: expression.to_string(),
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
    let inputs = expected_inputs(&subject);
    let reported_root = format!("target/ripr/rust-judged-panel/.staging-run-a/subjects/{case_id}");
    let mut finding = serde_json::json!({
        "id": format!("finding-{direction}"), "classification": class,
        "probe": {"family": producer_family, "file": format!("{reported_root}/src/lib.rs"), "line": 2, "expression": expression},
        "missing": missing, "recommended_next_step": recommendation
    });
    if let Some(kind) = limit {
        finding["static_limit_kind"] = Value::String(kind.to_string());
        finding["static_limitation"] = serde_json::json!({"kind": kind});
    }
    let report = serde_json::json!({
        "root": reported_root,
        "analysis_outcome": {"analysis_complete": true, "outcome": {"kind": "complete_with_findings", "limitations": []}},
        "findings": [finding]
    });
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ripr-packet-fixture-{}-{sequence}-{direction}",
        std::process::id()
    ));
    let materialized_root = root.join(case_id);
    fs::create_dir_all(materialized_root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        materialized_root.join("src/lib.rs"),
        format!("\nfn {owner}() {{ {expression}; }}\n"),
    )
    .map_err(|error| error.to_string())?;
    let case = ValidatedHostCase {
        case_id: case_id.to_string(),
        subject_id: case_id.to_string(),
        expected_direction: direction.to_string(),
        repository_base: "base".to_string(),
        repository_head: "head".to_string(),
        repository_tree: "tree".to_string(),
        argv: [
            "check",
            "--root",
            "<materialized-subject>",
            "--base",
            "base",
            "--mode",
            "draft",
            "--json",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect(),
        mode: "draft".to_string(),
        format: "json".to_string(),
        config_path: subject.config.repository_path.clone(),
        config_sha256: subject.config.sha256.clone(),
        diff_path: subject.diff.source_path.clone(),
        diff_sha256: subject.diff.sha256.clone(),
        executed_diff_identity: "sha256:executed".to_string(),
        subject_inputs: inputs
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
        receipt_ref: format!(
            "target/ripr/rust-judged-panel/runs/run-a/cases/{case_id}/receipt.json"
        ),
        receipt_sha256: "sha256:receipt".to_string(),
        stdout_ref: format!("target/ripr/rust-judged-panel/runs/run-a/cases/{case_id}/stdout.bin"),
        stdout_sha256: "sha256:stdout".to_string(),
        stderr_ref: format!("target/ripr/rust-judged-panel/runs/run-a/cases/{case_id}/stderr.bin"),
        stderr_sha256: "sha256:stderr".to_string(),
        stdout: serde_json::to_vec(&report).map_err(|error| error.to_string())?,
        materialized_root,
    };
    let host = ValidatedHostRun {
        current_ref: "target/ripr/rust-judged-panel/current.json".to_string(),
        current_sha256: "sha256:current".to_string(),
        index_ref: "target/ripr/rust-judged-panel/runs/run-a/run-index.json".to_string(),
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
    Ok(Fixture {
        _root: TestRoot(root),
        subject,
        case,
        host,
    })
}

fn projected(
    fixture: &Fixture,
) -> Result<(PortablePacket, RetainedAttestation, PortableIndexEntry), String> {
    let packet = project_one(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    let attestation =
        retained_attestation(&fixture.subject, &fixture.case, &fixture.host, "m", "s")?;
    let entry = PortableIndexEntry {
        case_id: packet.case_id.clone(),
        packet_path: "portable/packet.json".to_string(),
        packet_sha256: sha256_serialized(&packet)?,
        semantic_sha256: packet.semantic_sha256.clone(),
        attestation_path: "portable/attestation.json".to_string(),
        attestation_sha256: sha256_serialized(&attestation)?,
    };
    Ok((packet, attestation, entry))
}

fn rejects_reseal(fixture: &Fixture, mutate: fn(&mut PortablePacket)) -> Result<(), String> {
    rejects_structured_validate_at_reseal(
        "authority",
        &fixture.subject.case_id,
        |packet, _attestation| mutate(packet),
    )
}

fn rejects_direction_reseal(
    fixture: &Fixture,
    mutate: fn(&mut ObservedFinding),
) -> Result<(), String> {
    rejects_structured_validate_at_reseal(
        "direction",
        &fixture.subject.case_id,
        |packet, attestation| {
            mutate(&mut packet.semantic.observed);
            mutate(&mut attestation.semantic.observed);
        },
    )
}

fn rejects_validate_at_reseal(
    name: &str,
    case_id: &str,
    retain_member_paths: bool,
    prepare: impl FnOnce(
        &PortablePacket,
        &RetainedAttestation,
    ) -> Result<(Vec<u8>, String, Vec<u8>), String>,
) -> Result<(), String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest lacks repository parent".to_string())?;
    let root = TestRoot(scratch(name)?);
    copy_tree(
        &repository.join("metrics/rust-judged-behavior-panel"),
        &root.0.join("metrics/rust-judged-behavior-panel"),
    )?;
    let manifest = super::super::load_and_validate_at(&root.0, Path::new(MANIFEST_PATH))?;
    validate_at(&root.0, &manifest)?;
    let current_path = root.0.join(CURRENT_PATH);
    let mut current: PortableCurrent = read_strict_json(&current_path, "test current")?;
    let original_generation = root
        .0
        .join(&current.index_path)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "test current lacks generation".to_string())?;
    let mut index: PortableIndex =
        read_strict_json(&original_generation.join("packet-index.json"), "test index")?;
    let original_entry = index
        .packets
        .iter()
        .find(|entry| entry.case_id == case_id)
        .ok_or_else(|| format!("test index lacks `{case_id}`"))?
        .clone();
    let packet: PortablePacket =
        read_strict_json(&root.0.join(&original_entry.packet_path), "test packet")?;
    let attestation: RetainedAttestation = read_strict_json(
        &root.0.join(&original_entry.attestation_path),
        "test attestation",
    )?;
    let (packet_bytes, semantic_sha256, attestation_bytes) = prepare(&packet, &attestation)?;
    let entry = index
        .packets
        .iter_mut()
        .find(|entry| entry.case_id == case_id)
        .ok_or_else(|| format!("test index lacks `{case_id}`"))?;
    entry.semantic_sha256 = semantic_sha256;
    entry.packet_sha256 = sha256_bytes(&packet_bytes);
    entry.attestation_sha256 = sha256_bytes(&attestation_bytes);
    let next_id = generation_id(
        &index.manifest_sha256,
        &index.subjects_sha256,
        &index.packets,
    )?;
    index.generation_id = next_id.clone();
    for entry in &mut index.packets {
        if !retain_member_paths || entry.case_id != case_id {
            entry.packet_path = format!(
                "{PORTABLE_ROOT}/generations/{next_id}/packets/{}.json",
                entry.case_id
            );
            entry.attestation_path = format!(
                "{PORTABLE_ROOT}/generations/{next_id}/attestations/{}.json",
                entry.case_id
            );
        }
    }
    let next_generation = root
        .0
        .join(PORTABLE_ROOT)
        .join("generations")
        .join(&next_id);
    copy_tree(&original_generation, &next_generation)?;
    let changed_packet_path = next_generation
        .join("packets")
        .join(format!("{case_id}.json"));
    let changed_attestation_path = next_generation
        .join("attestations")
        .join(format!("{case_id}.json"));
    fs::write(changed_packet_path, packet_bytes).map_err(|error| error.to_string())?;
    fs::write(changed_attestation_path, attestation_bytes).map_err(|error| error.to_string())?;
    let index_bytes = pretty_json(&index)?;
    fs::write(next_generation.join("packet-index.json"), &index_bytes)
        .map_err(|error| error.to_string())?;
    current.generation_id = next_id.clone();
    current.index_path = format!("{PORTABLE_ROOT}/generations/{next_id}/packet-index.json");
    current.index_sha256 = sha256_bytes(&index_bytes);
    fs::write(&current_path, pretty_json(&current)?).map_err(|error| error.to_string())?;
    if validate_at(&root.0, &manifest).is_err() {
        Ok(())
    } else {
        Err(format!(
            "full packet/index/generation/current `{name}` reseal passed production validation"
        ))
    }
}

fn rejects_structured_validate_at_reseal(
    name: &str,
    case_id: &str,
    mutate: impl FnOnce(&mut PortablePacket, &mut RetainedAttestation),
) -> Result<(), String> {
    rejects_validate_at_reseal(name, case_id, false, |packet, attestation| {
        let mut packet = packet.clone();
        let mut attestation = attestation.clone();
        mutate(&mut packet, &mut attestation);
        packet.semantic_sha256 = sha256_serialized(&packet.semantic)?;
        Ok((
            pretty_json(&packet)?,
            packet.semantic_sha256.clone(),
            pretty_json(&attestation)?,
        ))
    })
}

#[test]
fn rust_judged_panel_packet_rejects_all_authority_family_reseals() -> Result<(), String> {
    let fixture = fixture("should_gap")?;
    for mutate in [
        (|p: &mut PortablePacket| p.semantic.producer_source_head = "stale".to_string())
            as fn(&mut PortablePacket),
        |p| p.semantic.producer_source_tree = "stale".to_string(),
        |p| p.semantic.producer_cargo_toml_sha256 = "stale".to_string(),
        |p| p.semantic.producer_cargo_lock_sha256 = "stale".to_string(),
        |p| p.semantic.producer_version = "stale".to_string(),
        |p| p.semantic.profile = "release".to_string(),
        |p| p.semantic.features.clear(),
        |p| p.semantic.argv = vec!["stale".to_string()],
        |p| p.semantic.mode = "stale".to_string(),
        |p| p.semantic.format = "stale".to_string(),
        |p| p.semantic.config_path = "stale".to_string(),
        |p| p.semantic.config_sha256 = "stale".to_string(),
        |p| p.semantic.diff_path = "stale".to_string(),
        |p| p.semantic.diff_sha256 = "stale".to_string(),
        |p| p.semantic.executed_diff_identity = "stale".to_string(),
        |p| p.semantic.subject_inputs.clear(),
        |p| p.semantic.observed.finding_id = "stale".to_string(),
        |p| p.semantic.observed.probe_family = "stale".to_string(),
        |p| p.semantic.observed.probe_file = "stale".to_string(),
        |p| p.semantic.observed.probe_line = 99,
        |p| p.semantic.observed.probe_expression = "stale".to_string(),
        |p| p.semantic.observed.static_limit_kind = Some("stale".to_string()),
        |p| p.host_evidence.host_target = "stale".to_string(),
        |p| p.host_evidence.binary_sha256 = "stale".to_string(),
        |p| p.host_evidence.run_id = "stale".to_string(),
        |p| p.host_evidence.receipt_sha256 = "stale".to_string(),
        |p| p.host_evidence.receipt_ref = "stale/receipt.json".to_string(),
        |p| p.host_evidence.current_ref = "stale/current.json".to_string(),
        |p| p.host_evidence.current_sha256 = "stale".to_string(),
        |p| p.host_evidence.index_ref = "stale/index.json".to_string(),
        |p| p.host_evidence.index_sha256 = "stale".to_string(),
        |p| p.host_evidence.stdout_ref = "stale/stdout.bin".to_string(),
        |p| p.host_evidence.stdout_sha256 = "stale".to_string(),
        |p| p.host_evidence.stderr_ref = "stale/stderr.bin".to_string(),
        |p| p.host_evidence.stderr_sha256 = "stale".to_string(),
        |p| p.host_evidence.analyzer_input_identity = "stale".to_string(),
    ] {
        rejects_reseal(&fixture, mutate)?;
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_validate_at_rejects_full_digest_chain_reseal() -> Result<(), String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest lacks repository parent".to_string())?;
    let root = std::env::temp_dir().join(format!("ripr-packet-reseal-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    copy_tree(
        &repository.join("metrics/rust-judged-behavior-panel"),
        &root.join("metrics/rust-judged-behavior-panel"),
    )?;
    let manifest = super::super::load_and_validate_at(&root, Path::new(MANIFEST_PATH))?;
    validate_at(&root, &manifest)?;
    let current_path = root.join(CURRENT_PATH);
    let mut current: PortableCurrent = read_strict_json(&current_path, "test current")?;
    let original_generation = root
        .join(&current.index_path)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "test current lacks generation".to_string())?;
    let mut index: PortableIndex =
        read_strict_json(&original_generation.join("packet-index.json"), "test index")?;
    let entry = index
        .packets
        .first_mut()
        .ok_or_else(|| "test index is empty".to_string())?;
    let mut packet: PortablePacket =
        read_strict_json(&root.join(&entry.packet_path), "test packet")?;
    packet.semantic.producer_source_head = "coordinated-stale-source".to_string();
    packet.semantic_sha256 = sha256_serialized(&packet.semantic)?;
    let packet_bytes = pretty_json(&packet)?;
    entry.semantic_sha256 = packet.semantic_sha256.clone();
    entry.packet_sha256 = sha256_bytes(&packet_bytes);
    let next_id = generation_id(
        &index.manifest_sha256,
        &index.subjects_sha256,
        &index.packets,
    )?;
    index.generation_id = next_id.clone();
    for entry in &mut index.packets {
        entry.packet_path = format!(
            "{PORTABLE_ROOT}/generations/{next_id}/packets/{}.json",
            entry.case_id
        );
        entry.attestation_path = format!(
            "{PORTABLE_ROOT}/generations/{next_id}/attestations/{}.json",
            entry.case_id
        );
    }
    let next_generation = root.join(PORTABLE_ROOT).join("generations").join(&next_id);
    copy_tree(&original_generation, &next_generation)?;
    let changed_entry = index
        .packets
        .first()
        .ok_or_else(|| "test index is empty".to_string())?;
    fs::write(root.join(&changed_entry.packet_path), packet_bytes)
        .map_err(|error| error.to_string())?;
    let index_bytes = pretty_json(&index)?;
    fs::write(next_generation.join("packet-index.json"), &index_bytes)
        .map_err(|error| error.to_string())?;
    current.generation_id = next_id.clone();
    current.index_path = format!("{PORTABLE_ROOT}/generations/{next_id}/packet-index.json");
    current.index_sha256 = sha256_bytes(&index_bytes);
    fs::write(&current_path, pretty_json(&current)?).map_err(|error| error.to_string())?;
    let rejected = validate_at(&root, &manifest).is_err();
    fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    if rejected {
        Ok(())
    } else {
        Err("full packet/index/generation/current reseal passed retained attestation".to_string())
    }
}

#[test]
fn rust_judged_panel_packet_rejects_resealed_stale_generation_member_paths() -> Result<(), String> {
    let fixture = fixture("should_gap")?;
    rejects_validate_at_reseal(
        "stale-member-path",
        &fixture.subject.case_id,
        true,
        |packet, attestation| {
            let mut packet = packet.clone();
            packet.semantic.producer_source_head = "next-generation".to_string();
            packet.semantic_sha256 = sha256_serialized(&packet.semantic)?;
            Ok((
                pretty_json(&packet)?,
                packet.semantic_sha256.clone(),
                pretty_json(attestation)?,
            ))
        },
    )
}

#[test]
fn rust_judged_panel_packet_binds_all_three_exact_direction_witnesses() -> Result<(), String> {
    for direction in ["should_gap", "should_stay_quiet", "should_limit"] {
        let fixture = fixture(direction)?;
        let (packet, attestation, entry) = projected(&fixture)?;
        validate_retained_packet(&packet, &entry, &attestation, &fixture.subject, "m", "s")?;
        rejects_direction_reseal(&fixture, |observed| {
            observed.recommendation.push_str("stale")
        })?;
        rejects_direction_reseal(&fixture, |observed| {
            observed.missing.push("stale".to_string())
        })?;
        rejects_direction_reseal(&fixture, |observed| {
            observed.static_limit_kind = Some("stale".to_string())
        })?;
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_strict_nested_readback_rejects_duplicate_and_unknown_keys()
-> Result<(), String> {
    fn rejects<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<(), String> {
        let text = std::str::from_utf8(body).map_err(|error| error.to_string())?;
        for hostile in [
            text.replacen('{', "{\n  \"unexpected\": true,", 1),
            text.replacen("\"kind\":", "\"kind\": \"duplicate\",\n  \"kind\":", 1),
        ] {
            if read_strict_json_bytes::<T>(hostile.as_bytes(), "hostile DTO").is_ok() {
                return Err("hostile DTO passed strict readback".to_string());
            }
        }
        Ok(())
    }
    let fixture = fixture("should_gap")?;
    let (packet, attestation, entry) = projected(&fixture)?;
    let current = PortableCurrent {
        schema_version: "0.1".to_string(),
        kind: "rust_judged_panel_portable_current".to_string(),
        generation_id: "generation".to_string(),
        index_path: "portable/index.json".to_string(),
        index_sha256: "sha256:index".to_string(),
    };
    let index = PortableIndex {
        schema_version: "0.1".to_string(),
        kind: "rust_judged_panel_portable_index".to_string(),
        publication_state: "complete".to_string(),
        generation_id: "generation".to_string(),
        manifest_sha256: "m".to_string(),
        subjects_sha256: "s".to_string(),
        packets: vec![entry],
        non_claims: expected_non_claims(),
    };
    rejects::<PortablePacket>(&pretty_json(&packet)?)?;
    rejects::<RetainedAttestation>(&pretty_json(&attestation)?)?;
    fn rejects_nested<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<(), String> {
        let text = std::str::from_utf8(body).map_err(|error| error.to_string())?;
        for hostile in [
            text.replacen(
                "\"host_evidence\": {",
                "\"host_evidence\": {\n    \"unexpected\": true,",
                1,
            ),
            text.replacen(
                "\"availability\":",
                "\"availability\": \"duplicate\",\n    \"availability\":",
                1,
            ),
        ] {
            if read_strict_json_bytes::<T>(hostile.as_bytes(), "hostile nested DTO").is_ok() {
                return Err("hostile nested host DTO passed strict readback".to_string());
            }
        }
        Ok(())
    }
    rejects_nested::<PortablePacket>(&pretty_json(&packet)?)?;
    rejects_nested::<RetainedAttestation>(&pretty_json(&attestation)?)?;
    rejects::<PortableIndex>(&pretty_json(&index)?)?;
    rejects::<PortableCurrent>(&pretty_json(&current)?)?;
    let case_id = fixture.subject.case_id.clone();
    for (name, needle, replacement) in [
        (
            "nested-unknown",
            "\"host_evidence\": {",
            "\"host_evidence\": {\n    \"unexpected\": true,",
        ),
        (
            "nested-duplicate",
            "\"availability\":",
            "\"availability\": \"duplicate\",\n    \"availability\":",
        ),
    ] {
        rejects_validate_at_reseal(name, &case_id, false, |packet, attestation| {
            let packet_bytes = pretty_json(packet)?;
            let hostile_packet = std::str::from_utf8(&packet_bytes)
                .map_err(|error| error.to_string())?
                .replacen(needle, replacement, 1)
                .into_bytes();
            Ok((
                hostile_packet,
                packet.semantic_sha256.clone(),
                pretty_json(attestation)?,
            ))
        })?;
        rejects_validate_at_reseal(name, &case_id, false, |packet, attestation| {
            let attestation_bytes = pretty_json(attestation)?;
            let hostile_attestation = std::str::from_utf8(&attestation_bytes)
                .map_err(|error| error.to_string())?
                .replacen(needle, replacement, 1)
                .into_bytes();
            Ok((
                pretty_json(packet)?,
                packet.semantic_sha256.clone(),
                hostile_attestation,
            ))
        })?;
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn rust_judged_panel_packet_rejects_in_repo_sibling_symlink() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("ripr-packet-link-{}", std::process::id()));
    let portable = root.join(PORTABLE_ROOT);
    let sibling = root.join("metrics/rust-judged-behavior-panel/sibling");
    let outside =
        std::env::temp_dir().join(format!("ripr-packet-link-outside-{}", std::process::id()));
    fs::create_dir_all(&portable).map_err(|error| error.to_string())?;
    fs::create_dir_all(&sibling).map_err(|error| error.to_string())?;
    fs::write(sibling.join("packet.json"), b"outside").map_err(|error| error.to_string())?;
    fs::create_dir_all(&outside).map_err(|error| error.to_string())?;
    fs::write(outside.join("packet.json"), b"outside").map_err(|error| error.to_string())?;
    let actual = portable.join("actual");
    fs::create_dir_all(&actual).map_err(|error| error.to_string())?;
    fs::write(actual.join("packet.json"), b"inside").map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&sibling, portable.join("linked"))
        .map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&outside, portable.join("outside"))
        .map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&actual, portable.join("alias"))
        .map_err(|error| error.to_string())?;
    let rejected =
        require_canonical_portable(&root, &portable.join("linked/packet.json"), "packet").is_err();
    let outside_rejected =
        require_canonical_portable(&root, &portable.join("outside/packet.json"), "packet").is_err();
    let alias_rejected =
        require_canonical_portable(&root, &portable.join("alias/packet.json"), "packet").is_err();
    fs::remove_dir_all(&portable).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&sibling, &portable).map_err(|error| error.to_string())?;
    let fixtures = ["should_gap", "should_stay_quiet", "should_limit"]
        .into_iter()
        .map(fixture)
        .collect::<Result<Vec<_>, _>>()?;
    let packets = fixtures
        .iter()
        .map(|fixture| projected(fixture).map(|value| value.0))
        .collect::<Result<Vec<_>, _>>()?;
    let attestations = fixtures
        .iter()
        .map(|fixture| {
            retained_attestation(&fixture.subject, &fixture.case, &fixture.host, "m", "s")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let write_rejected = publish_all(&root, "m", "s", &packets, &attestations, None).is_err();
    let sibling_unchanged =
        !sibling.join("current.json").exists() && !sibling.join("generations").exists();
    fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    fs::remove_dir_all(outside).map_err(|error| error.to_string())?;
    if rejected && outside_rejected && alias_rejected && write_rejected && sibling_unchanged {
        Ok(())
    } else {
        Err("portable sibling symlink escaped canonical root".to_string())
    }
}

#[test]
fn rust_judged_panel_packet_rejects_noncanonical_portable_spellings() -> Result<(), String> {
    for path in [
        "metrics\\rust-judged-behavior-panel\\portable\\current.json",
        "metrics/rust-judged-behavior-panel/portable//current.json",
        "metrics/rust-judged-behavior-panel/portable/../portable/current.json",
    ] {
        if safe_portable_path(path).is_ok() {
            return Err(format!("noncanonical portable spelling passed: `{path}`"));
        }
    }
    Ok(())
}

#[test]
fn rust_judged_panel_packet_reuse_validates_every_member_before_current() -> Result<(), String> {
    let fixtures = ["should_gap", "should_stay_quiet", "should_limit"]
        .into_iter()
        .map(fixture)
        .collect::<Result<Vec<_>, _>>()?;
    let projected = fixtures
        .iter()
        .map(projected)
        .collect::<Result<Vec<_>, _>>()?;
    let packets = projected
        .iter()
        .map(|(packet, _, _)| packet.clone())
        .collect::<Vec<_>>();
    let attestations = projected
        .iter()
        .map(|(_, attestation, _)| attestation.clone())
        .collect::<Vec<_>>();
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = TestRoot(std::env::temp_dir().join(format!(
        "ripr-packet-reuse-{}-{sequence}",
        std::process::id()
    )));
    fs::create_dir_all(&root.0).map_err(|error| error.to_string())?;
    publish_all(
        &root.0,
        "sha256:m",
        "sha256:s",
        &packets,
        &attestations,
        None,
    )?;
    let current_path = root.0.join(CURRENT_PATH);
    let prior = fs::read(&current_path).map_err(|error| error.to_string())?;
    let current: PortableCurrent = read_strict_json(&current_path, "test current")?;
    let index: PortableIndex = read_strict_json(&root.0.join(&current.index_path), "test index")?;
    let entry = index
        .packets
        .first()
        .ok_or_else(|| "test index is empty".to_string())?;
    fs::remove_file(root.0.join(&entry.packet_path)).map_err(|error| error.to_string())?;
    if publish_all(
        &root.0,
        "sha256:m",
        "sha256:s",
        &packets,
        &attestations,
        None,
    )
    .is_ok()
        || fs::read(&current_path).map_err(|error| error.to_string())? != prior
    {
        return Err("tampered reuse advanced or changed the prior current".to_string());
    }
    Ok(())
}

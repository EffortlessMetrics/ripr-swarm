use super::{
    MANIFEST_PATH, RustJudgedPanelItem, load_and_validate_at, parse_json_without_duplicate_keys,
};
use crate::run::{TimedOutput, capture_output_in_dir_with_timeout};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCHEMA_VERSION: &str = "0.1";
const KIND: &str = "rust_judged_panel_replay_packet";
const PROCESS_TIMEOUT: Duration = Duration::from_mins(1);
const FIXED_GIT_DATE: &str = "2000-01-01T00:00:00Z";

#[derive(Deserialize, Serialize)]
struct ReplayPacket {
    schema_version: String,
    kind: String,
    case_id: String,
    expected_direction: String,
    seed_manifest_sha256: String,
    selected_row_sha256: String,
    diff_sha256: String,
    repository_fixture_id: String,
    base_sha: String,
    head_sha: String,
    tree_sha: String,
    source_file: String,
    source_sha256: String,
    test_files: Vec<FileIdentity>,
    ripr_binary_path: String,
    ripr_binary_sha256: String,
    ripr_version: String,
    build_profile: String,
    enabled_features: Vec<String>,
    config_path: String,
    config_sha256: String,
    analysis_profile: String,
    argv: Vec<String>,
    analyzer_input_identity: Option<String>,
    execution: ExecutionRecord,
    raw_stdout_sha256: String,
    raw_stderr_sha256: String,
    raw_stdout_ref: String,
    raw_stderr_ref: String,
    selected_projection: Option<Value>,
    judgment: Value,
    runtime_calibration: Value,
    limitations: Vec<String>,
    non_claims: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    packet_digest: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct FileIdentity {
    path: String,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
struct ExecutionRecord {
    disposition: String,
    exit_code: i32,
    timed_out: bool,
    analysis_complete: bool,
    error: Option<String>,
}

struct Fixture<'a> {
    old_source: &'a str,
    new_source: &'a str,
    tests: &'a str,
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let (ripr_bin, out) = parse_args(args)?;
    let root =
        std::env::current_dir().map_err(|error| format!("resolve repository root: {error}"))?;
    let manifest_path = root.join(MANIFEST_PATH);
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("read {}: {error}", manifest_path.display()))?;
    let manifest = load_and_validate_at(&root, Path::new(MANIFEST_PATH))?;
    let manifest_value = parse_json_without_duplicate_keys(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|error| format!("manifest is not UTF-8: {error}"))?,
    )
    .map_err(|error| format!("parse manifest for row identity: {error}"))?;
    let binary = canonical_binary(&root, &ripr_bin)?;
    let binary_digest = sha256_file(&binary)?;
    let version = run_version(&binary)?;
    fs::create_dir_all(&out).map_err(|error| format!("create {}: {error}", out.display()))?;
    let scratch = out.join(format!(".scratch-{}", std::process::id()));
    if scratch.exists() {
        return Err(format!(
            "refuse to replace existing replay scratch {}",
            scratch.display()
        ));
    }
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("create replay scratch {}: {error}", scratch.display()))?;

    for item in &manifest.items {
        let row = manifest_value
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|row| row.get("id").and_then(Value::as_str) == Some(&item.id))
            })
            .ok_or_else(|| format!("validated item `{}` was absent from raw manifest", item.id))?;
        replay_case(
            &root,
            &scratch,
            &out,
            &binary,
            &binary_digest,
            &version,
            &manifest_bytes,
            row,
            item,
        )?;
    }
    fs::remove_dir_all(&scratch)
        .map_err(|error| format!("remove replay scratch {}: {error}", scratch.display()))?;
    println!(
        "Rust judged panel replay complete: cases={} out={}",
        manifest.items.len(),
        out.display()
    );
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let mut ripr_bin = None;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--ripr-bin" => {
                index += 1;
                ripr_bin = args.get(index).map(PathBuf::from);
            }
            "--out" => {
                index += 1;
                out = args.get(index).map(PathBuf::from);
            }
            other => {
                return Err(format!(
                    "unknown rust-judged-panel replay argument `{other}`"
                ));
            }
        }
        index += 1;
    }
    Ok((
        ripr_bin.ok_or_else(|| "replay requires --ripr-bin <path>".to_string())?,
        out.ok_or_else(|| "replay requires --out <path>".to_string())?,
    ))
}

#[allow(
    clippy::too_many_arguments,
    reason = "one replay case binds the selected authorities explicitly"
)]
fn replay_case(
    root: &Path,
    scratch: &Path,
    out: &Path,
    binary: &Path,
    binary_digest: &str,
    version: &str,
    manifest_bytes: &[u8],
    row: &Value,
    item: &RustJudgedPanelItem,
) -> Result<(), String> {
    let fixture = fixture_for(&item.expected_direction)?;
    let case_root = scratch.join(&item.id);
    let diff_source = root.join(&item.diff_path);
    let diff_bytes = fs::read(&diff_source)
        .map_err(|error| format!("read selected diff {}: {error}", diff_source.display()))?;
    materialize_repository(&case_root, &fixture, &diff_bytes)?;
    let base_sha = git(&case_root, &["rev-parse", "HEAD^"])?;
    let head_sha = git(&case_root, &["rev-parse", "HEAD"])?;
    let tree_sha = git(&case_root, &["rev-parse", "HEAD^{tree}"])?;
    fs::write(case_root.join("selected.diff"), &diff_bytes)
        .map_err(|error| format!("write selected replay diff: {error}"))?;

    let argv = vec![
        "check".to_string(),
        "--root".to_string(),
        ".".to_string(),
        "--diff".to_string(),
        "selected.diff".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--mode".to_string(),
        "draft".to_string(),
    ];
    let output = capture_output_in_dir_with_timeout(
        binary.to_string_lossy().as_ref(),
        &argv,
        &[],
        Some(&case_root),
        PROCESS_TIMEOUT,
        &format!("replay case {}", item.id),
    )
    .unwrap_or_else(|error| TimedOutput {
        status: None,
        stdout: String::new(),
        stderr: error,
        duration: Duration::ZERO,
        timed_out: false,
    });
    let status = output.status;
    let execution_error = if output.timed_out {
        Some("process_timeout".to_string())
    } else if status.is_none() {
        Some("spawn_or_wait_failure".to_string())
    } else if !status.is_some_and(|value| value.success()) {
        Some("nonzero_exit".to_string())
    } else {
        None
    };
    let parsed = if execution_error.is_none() {
        serde_json::from_str::<Value>(&output.stdout).map_err(|error| error.to_string())
    } else {
        Err(execution_error.clone().unwrap_or_default())
    };
    let parsed_error = parsed
        .as_ref()
        .err()
        .map(|error| format!("malformed_json: {error}"));
    let parsed = parsed.ok();
    let analysis_complete = parsed
        .as_ref()
        .and_then(|value| value.pointer("/analysis_outcome/analysis_complete"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let input_identity = parsed.as_ref().and_then(|value| {
        value
            .pointer("/analysis_outcome/outcome/identity/input_identity")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    });
    let expected_input_identity = sha256(&diff_bytes);
    let identity_error = (input_identity.as_deref() != Some(expected_input_identity.as_str()))
        .then(|| "stale_analyzer_input_identity".to_string());
    let projection = parsed
        .as_ref()
        .and_then(|value| select_projection(item, value).ok());
    let projection_error = (projection.is_none() && parsed.is_some())
        .then(|| "selected_projection_mismatch".to_string());
    let error = execution_error
        .or(parsed_error)
        .or_else(|| (!analysis_complete).then(|| "incomplete_analysis".to_string()))
        .or(identity_error)
        .or(projection_error);
    let case_out = out.join(&item.id);
    fs::create_dir_all(&case_out)
        .map_err(|error| format!("create case output {}: {error}", case_out.display()))?;
    atomic_write(&case_out.join("stdout.json"), output.stdout.as_bytes())?;
    atomic_write(&case_out.join("stderr.txt"), output.stderr.as_bytes())?;

    let source = fs::read(case_root.join("src/lib.rs"))
        .map_err(|error| format!("read materialized source: {error}"))?;
    let tests = fs::read(case_root.join("tests/replay.rs"))
        .map_err(|error| format!("read materialized tests: {error}"))?;
    let mut packet = ReplayPacket {
        schema_version: SCHEMA_VERSION.to_string(),
        kind: KIND.to_string(),
        case_id: item.id.clone(),
        expected_direction: item.expected_direction.clone(),
        seed_manifest_sha256: sha256(manifest_bytes),
        selected_row_sha256: sha256(&serde_json::to_vec(row).map_err(|error| error.to_string())?),
        diff_sha256: sha256(&diff_bytes),
        repository_fixture_id: item.repository.clone(),
        base_sha,
        head_sha,
        tree_sha,
        source_file: "src/lib.rs".to_string(),
        source_sha256: sha256(&source),
        test_files: vec![FileIdentity {
            path: "tests/replay.rs".to_string(),
            sha256: sha256(&tests),
        }],
        ripr_binary_path: "<workspace-ripr-bin>".to_string(),
        ripr_binary_sha256: binary_digest.to_string(),
        ripr_version: version.to_string(),
        build_profile: "supplied_workspace_binary".to_string(),
        enabled_features: Vec::new(),
        config_path: "ripr.toml".to_string(),
        config_sha256: sha256_file(&case_root.join("ripr.toml"))?,
        analysis_profile: "draft".to_string(),
        argv: argv.clone(),
        analyzer_input_identity: input_identity,
        execution: ExecutionRecord {
            disposition: if error.is_none() {
                "complete"
            } else {
                "failed"
            }
            .to_string(),
            exit_code: status.and_then(|value| value.code()).unwrap_or(-1),
            timed_out: output.timed_out,
            analysis_complete,
            error: error.clone(),
        },
        raw_stdout_sha256: sha256(output.stdout.as_bytes()),
        raw_stderr_sha256: sha256(output.stderr.as_bytes()),
        raw_stdout_ref: format!("{}/stdout.json", item.id),
        raw_stderr_ref: format!("{}/stderr.txt", item.id),
        selected_projection: projection,
        judgment: json!({"status":"unjudged","labels":null}),
        runtime_calibration: json!({"status":"not_run","outcome":null,"evidence_ref":null}),
        limitations: parsed
            .as_ref()
            .and_then(|value| value.pointer("/analysis_outcome/outcome/limitations"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        non_claims: item.must_not_claim.clone(),
        packet_digest: None,
    };
    packet.packet_digest = Some(packet_identity_digest(&packet)?);
    let packet_bytes = serde_json::to_vec_pretty(&packet).map_err(|error| error.to_string())?;
    validate_packet_bytes(
        &packet_bytes,
        output.stdout.as_bytes(),
        output.stderr.as_bytes(),
        &sha256(manifest_bytes),
        &sha256(&serde_json::to_vec(row).map_err(|error| error.to_string())?),
        &sha256(&diff_bytes),
        binary_digest,
        &sha256_file(&case_root.join("ripr.toml"))?,
    )?;
    atomic_write(&case_out.join("packet.json"), &packet_bytes)?;
    if let Some(error) = error {
        return Err(format!("replay case `{}` failed closed: {error}", item.id));
    }
    Ok(())
}

fn select_projection(item: &RustJudgedPanelItem, output: &Value) -> Result<Value, String> {
    let findings = output
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("case `{}` omitted findings array", item.id))?;
    let matching = findings
        .iter()
        .filter(|finding| {
            finding
                .pointer("/probe/file")
                .and_then(Value::as_str)
                .is_some_and(|file| file.ends_with(&item.anchor.file))
                && finding.pointer("/probe/line").and_then(Value::as_u64) == Some(item.anchor.line)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "case `{}` requires exactly one anchor join, found {}",
            item.id,
            matching.len()
        ));
    }
    let finding = matching[0];
    let probe_expression = finding
        .pointer("/probe/expression")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("case `{}` omitted the selected probe expression", item.id))?;
    if probe_expression != item.anchor.changed_behavior {
        return Err(format!(
            "case `{}` selected stale changed behavior `{probe_expression}`",
            item.id
        ));
    }
    let classification = finding
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("");
    if classification != item.expected_classification {
        return Err(format!(
            "case `{}` expected classification `{}`, found `{classification}`",
            item.id, item.expected_classification
        ));
    }
    if item.expected_direction != "should_limit" {
        let related = finding
            .get("related_tests")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("case `{}` omitted related-test witnesses", item.id))?;
        if !related.iter().any(|test| {
            test.get("relation_reason").and_then(Value::as_str)
                == Some(item.test_evidence.relation_basis.as_str())
                && test.get("oracle_kind").and_then(Value::as_str)
                    == Some(item.test_evidence.oracle_kind.as_str())
                && test.get("oracle_strength").and_then(Value::as_str)
                    == Some(item.test_evidence.oracle_strength.as_str())
        }) {
            return Err(format!(
                "case `{}` omitted its canonical relation/oracle witness",
                item.id
            ));
        }
    }
    if let Some(limit_kind) = item.expected_static_limit_kind.value()
        && finding.get("static_limit_kind").and_then(Value::as_str) != Some(limit_kind.as_str())
    {
        return Err(format!(
            "case `{}` omitted static limit `{limit_kind}`",
            item.id
        ));
    }
    match item.expected_direction.as_str() {
        "should_gap"
            if !finding
                .to_string()
                .contains(&item.anchor.required_discriminator) =>
        {
            return Err(format!("case `{}` omitted required discriminator", item.id));
        }
        "should_stay_quiet" if finding.to_string().contains("missing discriminator") => {
            return Err(format!(
                "case `{}` retained a missing discriminator",
                item.id
            ));
        }
        "should_limit" if classification != "no_static_path" => {
            return Err(format!(
                "case `{}` did not retain its static limitation",
                item.id
            ));
        }
        _ => {}
    }
    Ok((*finding).clone())
}

fn fixture_for(direction: &str) -> Result<Fixture<'static>, String> {
    const OLD_BOUNDARY: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount > threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
    const NEW_BOUNDARY: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
    match direction {
        "should_gap" => Ok(Fixture {
            old_source: OLD_BOUNDARY,
            new_source: NEW_BOUNDARY,
            tests: "use replay_case::discounted_total;\n#[test]\nfn below() { assert_eq!(discounted_total(50, 100), 50); }\n#[test]\nfn above() { assert_eq!(discounted_total(10_000, 100), 9_990); }\n",
        }),
        "should_stay_quiet" => Ok(Fixture {
            old_source: OLD_BOUNDARY,
            new_source: NEW_BOUNDARY,
            tests: "use replay_case::discounted_total;\n#[test]\nfn equality() { assert_eq!(discounted_total(100, 100), 90); }\n",
        }),
        "should_limit" => Ok(Fixture {
            old_source: "pub fn normalize_score(value: i32) -> i32 {\n    value.max(0)\n}\n",
            new_source: "pub fn normalize_score(value: i32) -> i32 {\n    value.max(1)\n}\n",
            tests: "use replay_case::normalize_score;\nmacro_rules! check_score { ($value:expr, $expected:expr) => { assert_eq!(normalize_score($value), $expected); }; }\n#[test]\nfn macro_wrapped() { check_score!(0, 1); }\n",
        }),
        other => Err(format!("unsupported replay direction `{other}`")),
    }
}

fn materialize_repository(root: &Path, fixture: &Fixture<'_>, diff: &[u8]) -> Result<(), String> {
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"replay-case\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.join("src/lib.rs"), fixture.old_source).map_err(|error| error.to_string())?;
    fs::write(root.join("tests/replay.rs"), fixture.tests).map_err(|error| error.to_string())?;
    fs::write(root.join("ripr.toml"), "[analysis]\nmode = \"draft\"\n")
        .map_err(|error| error.to_string())?;
    git(root, &["init", "-q"])?;
    git(root, &["config", "core.autocrlf", "false"])?;
    git(root, &["config", "user.name", "RIPR Replay"])?;
    git(root, &["config", "user.email", "replay@example.invalid"])?;
    git(
        root,
        &[
            "add",
            "Cargo.toml",
            "ripr.toml",
            "src/lib.rs",
            "tests/replay.rs",
        ],
    )?;
    git_commit(root, "base")?;
    fs::write(root.join("selected.diff"), diff).map_err(|error| error.to_string())?;
    git(root, &["apply", "--check", "selected.diff"])?;
    git(root, &["apply", "selected.diff"])?;
    let applied = fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
    if applied != fixture.new_source {
        return Err("selected diff post-image does not match governed fixture".to_string());
    }
    git(root, &["add", "src/lib.rs"])?;
    git_commit(root, "head")?;
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut owned = vec!["-C".to_string(), root.to_string_lossy().into_owned()];
    owned.extend(args.iter().map(|value| (*value).to_string()));
    let output = capture_output_in_dir_with_timeout(
        "git",
        &owned,
        &[],
        None,
        Duration::from_secs(15),
        "Rust judged panel Git fixture",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            output.stderr.trim()
        ));
    }
    Ok(output.stdout.trim().to_string())
}

fn git_commit(root: &Path, message: &str) -> Result<(), String> {
    let mut owned = vec!["-C".to_string(), root.to_string_lossy().into_owned()];
    owned.extend(
        ["commit", "-qm", message]
            .iter()
            .map(|value| (*value).to_string()),
    );
    let envs = [
        ("GIT_AUTHOR_DATE", FIXED_GIT_DATE),
        ("GIT_COMMITTER_DATE", FIXED_GIT_DATE),
    ];
    let output = capture_output_in_dir_with_timeout(
        "git",
        &owned,
        &envs,
        None,
        Duration::from_secs(15),
        "Rust judged panel Git commit",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!("git commit failed: {}", output.stderr.trim()));
    }
    Ok(())
}

fn canonical_binary(root: &Path, binary: &Path) -> Result<PathBuf, String> {
    let path = if binary.is_absolute() {
        binary.to_path_buf()
    } else {
        root.join(binary)
    };
    let canonical = fs::canonicalize(&path)
        .map_err(|error| format!("resolve supplied RIPR binary {}: {error}", path.display()))?;
    if !canonical.is_file() {
        return Err(format!(
            "supplied RIPR binary is not a file: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn run_version(binary: &Path) -> Result<String, String> {
    let output = capture_output_in_dir_with_timeout(
        binary.to_string_lossy().as_ref(),
        &["--version".to_string()],
        &[],
        None,
        Duration::from_secs(15),
        "RIPR replay version probe",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "supplied RIPR binary version probe failed: {}",
            output.stderr.trim()
        ));
    }
    Ok(output.stdout.trim().to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| sha256(&bytes))
        .map_err(|error| format!("read {}: {error}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn packet_identity_digest(packet: &ReplayPacket) -> Result<String, String> {
    let mut identity = serde_json::to_value(packet).map_err(|error| error.to_string())?;
    identity
        .as_object_mut()
        .ok_or_else(|| "replay packet is not an object".to_string())?
        .remove("packet_digest");
    serde_json::to_vec(&identity)
        .map(|bytes| sha256(&bytes))
        .map_err(|error| error.to_string())
}

#[allow(
    clippy::too_many_arguments,
    reason = "packet validation binds every independent evidence authority"
)]
fn validate_packet_bytes(
    packet_bytes: &[u8],
    stdout: &[u8],
    stderr: &[u8],
    manifest_digest: &str,
    row_digest: &str,
    diff_digest: &str,
    binary_digest: &str,
    config_digest: &str,
) -> Result<(), String> {
    let packet: ReplayPacket = serde_json::from_slice(packet_bytes)
        .map_err(|error| format!("parse replay packet: {error}"))?;
    let expected_packet_digest = packet
        .packet_digest
        .as_deref()
        .ok_or_else(|| "replay packet omitted self digest".to_string())?;
    if packet_identity_digest(&packet)? != expected_packet_digest {
        return Err("replay packet self digest mismatch".to_string());
    }
    let stdout_digest = sha256(stdout);
    let stderr_digest = sha256(stderr);
    let checks = [
        (
            packet.seed_manifest_sha256.as_str(),
            manifest_digest,
            "manifest",
        ),
        (
            packet.selected_row_sha256.as_str(),
            row_digest,
            "selected row",
        ),
        (packet.diff_sha256.as_str(), diff_digest, "selected diff"),
        (
            packet.ripr_binary_sha256.as_str(),
            binary_digest,
            "RIPR binary",
        ),
        (packet.config_sha256.as_str(), config_digest, "config"),
        (
            packet.raw_stdout_sha256.as_str(),
            stdout_digest.as_str(),
            "raw stdout",
        ),
        (
            packet.raw_stderr_sha256.as_str(),
            stderr_digest.as_str(),
            "raw stderr",
        ),
    ];
    for (actual, expected, label) in checks {
        if actual != expected {
            return Err(format!("replay packet {label} digest mismatch"));
        }
    }
    if packet.execution.disposition == "complete"
        && packet.analyzer_input_identity.as_deref() != Some(diff_digest)
    {
        return Err("replay packet analyzer input identity mismatch".to_string());
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("publish {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealed_test_packet(stdout: &[u8], stderr: &[u8]) -> Result<Vec<u8>, String> {
        let mut packet = ReplayPacket {
            schema_version: SCHEMA_VERSION.to_string(),
            kind: KIND.to_string(),
            case_id: "alternate-case".to_string(),
            expected_direction: "should_gap".to_string(),
            seed_manifest_sha256: "manifest".to_string(),
            selected_row_sha256: "row".to_string(),
            diff_sha256: "diff".to_string(),
            repository_fixture_id: "alternate-repository".to_string(),
            base_sha: "base".to_string(),
            head_sha: "head".to_string(),
            tree_sha: "tree".to_string(),
            source_file: "src/lib.rs".to_string(),
            source_sha256: "source".to_string(),
            test_files: Vec::new(),
            ripr_binary_path: "<workspace-ripr-bin>".to_string(),
            ripr_binary_sha256: "binary".to_string(),
            ripr_version: "ripr test".to_string(),
            build_profile: "test".to_string(),
            enabled_features: Vec::new(),
            config_path: "ripr.toml".to_string(),
            config_sha256: "config".to_string(),
            analysis_profile: "draft".to_string(),
            argv: Vec::new(),
            analyzer_input_identity: Some("diff".to_string()),
            execution: ExecutionRecord {
                disposition: "complete".to_string(),
                exit_code: 0,
                timed_out: false,
                analysis_complete: true,
                error: None,
            },
            raw_stdout_sha256: sha256(stdout),
            raw_stderr_sha256: sha256(stderr),
            raw_stdout_ref: "alternate-case/stdout.json".to_string(),
            raw_stderr_ref: "alternate-case/stderr.txt".to_string(),
            selected_projection: Some(json!({"id":"selected"})),
            judgment: json!({"status":"unjudged","labels":null}),
            runtime_calibration: json!({"status":"not_run","outcome":null,"evidence_ref":null}),
            limitations: Vec::new(),
            non_claims: Vec::new(),
            packet_digest: None,
        };
        packet.packet_digest = Some(packet_identity_digest(&packet)?);
        serde_json::to_vec_pretty(&packet).map_err(|error| error.to_string())
    }

    #[test]
    fn packet_validator_rejects_tampered_raw_and_stale_authorities() -> Result<(), String> {
        let stdout = br#"{"ok":true}"#;
        let packet = sealed_test_packet(stdout, b"")?;
        validate_packet_bytes(
            &packet, stdout, b"", "manifest", "row", "diff", "binary", "config",
        )?;
        if validate_packet_bytes(
            &packet,
            b"tampered",
            b"",
            "manifest",
            "row",
            "diff",
            "binary",
            "config",
        )
        .is_ok()
        {
            return Err("tampered raw stdout was accepted".to_string());
        }
        if validate_packet_bytes(
            &packet,
            stdout,
            b"",
            "manifest",
            "stale-row",
            "diff",
            "binary",
            "config",
        )
        .is_ok()
        {
            return Err("stale selected-row authority was accepted".to_string());
        }
        if validate_packet_bytes(
            &packet,
            stdout,
            b"",
            "manifest",
            "row",
            "diff",
            "stale-binary",
            "config",
        )
        .is_ok()
        {
            return Err("stale binary authority was accepted".to_string());
        }
        let mut tampered: Value =
            serde_json::from_slice(&packet).map_err(|error| error.to_string())?;
        tampered["config_sha256"] = Value::String("changed".to_string());
        let bytes = serde_json::to_vec(&tampered).map_err(|error| error.to_string())?;
        if validate_packet_bytes(
            &bytes, stdout, b"", "manifest", "row", "diff", "binary", "changed",
        )
        .is_ok()
        {
            return Err("stale packet self digest was accepted".to_string());
        }
        tampered
            .as_object_mut()
            .ok_or_else(|| "test packet is not an object".to_string())?
            .remove("packet_digest");
        let bytes = serde_json::to_vec(&tampered).map_err(|error| error.to_string())?;
        if validate_packet_bytes(
            &bytes, stdout, b"", "manifest", "row", "diff", "binary", "changed",
        )
        .is_ok()
        {
            return Err("packet without its integrity grip was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn failed_execution_retains_raw_streams_and_failure_packet() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "xtask manifest lacks repository parent".to_string())?;
        let manifest_bytes =
            fs::read(root.join(MANIFEST_PATH)).map_err(|error| error.to_string())?;
        let manifest = load_and_validate_at(root, Path::new(MANIFEST_PATH))?;
        let item = manifest
            .items
            .first()
            .ok_or_else(|| "panel is empty".to_string())?;
        let manifest_value = parse_json_without_duplicate_keys(
            std::str::from_utf8(&manifest_bytes).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let row = manifest_value
            .pointer("/items/0")
            .ok_or_else(|| "first raw row is absent".to_string())?;
        let out = root.join("target/ripr/rust-judged-panel-failure-oracle");
        let scratch = out.join(format!("scratch-{}", std::process::id()));
        if out.exists() {
            fs::remove_dir_all(&out).map_err(|error| error.to_string())?;
        }
        fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        let failing_binary = Path::new(env!("COMSPEC"));
        #[cfg(not(windows))]
        let failing_binary = Path::new("/bin/false");
        let result = replay_case(
            root,
            &scratch,
            &out,
            failing_binary,
            "test-binary",
            "failing-test-binary",
            &manifest_bytes,
            row,
            item,
        );
        if result.is_ok() {
            return Err("failing analyzer was accepted".to_string());
        }
        let case_out = out.join(&item.id);
        let stdout = fs::read(case_out.join("stdout.json")).map_err(|error| error.to_string())?;
        let stderr = fs::read(case_out.join("stderr.txt")).map_err(|error| error.to_string())?;
        let packet_bytes =
            fs::read(case_out.join("packet.json")).map_err(|error| error.to_string())?;
        let packet: ReplayPacket =
            serde_json::from_slice(&packet_bytes).map_err(|error| error.to_string())?;
        if packet.execution.disposition != "failed" || packet.execution.error.is_none() {
            return Err("failure packet omitted its explicit disposition".to_string());
        }
        validate_packet_bytes(
            &packet_bytes,
            &stdout,
            &stderr,
            &sha256(&manifest_bytes),
            &sha256(&serde_json::to_vec(row).map_err(|error| error.to_string())?),
            &sha256_file(&root.join(&item.diff_path))?,
            "test-binary",
            &sha256_file(&scratch.join(&item.id).join("ripr.toml"))?,
        )?;
        fs::remove_dir_all(&out).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn fixtures_cover_the_closed_direction_denominator() -> Result<(), String> {
        for direction in ["should_gap", "should_stay_quiet", "should_limit"] {
            let fixture = fixture_for(direction)?;
            if fixture.old_source == fixture.new_source || fixture.tests.trim().is_empty() {
                return Err(format!("fixture `{direction}` is not discriminating"));
            }
        }
        if fixture_for("neighboring_case").is_ok() {
            return Err("unknown direction acquired an implicit fixture".to_string());
        }
        Ok(())
    }

    #[test]
    fn projection_rejects_neighboring_or_multiple_anchor_matches() -> Result<(), String> {
        let fixture = fixture_for("should_gap")?;
        if !fixture.tests.contains("10_000") {
            return Err("gap alternate input disappeared".to_string());
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "xtask manifest lacks repository parent".to_string())?;
        let manifest = load_and_validate_at(root, Path::new(MANIFEST_PATH))?;
        let item = manifest
            .items
            .iter()
            .find(|item| item.expected_direction == "should_gap")
            .ok_or_else(|| "validated manifest omitted should_gap".to_string())?;
        let output = json!({"findings":[
            {"classification":"weakly_exposed","probe":{"file":"src/lib.rs","line":2},"missing":["amount == threshold"]},
            {"classification":"weakly_exposed","probe":{"file":"src/lib.rs","line":2},"missing":["amount == threshold"]}
        ]});
        if select_projection(item, &output).is_ok() {
            return Err("multiple exact joins were accepted".to_string());
        }
        Ok(())
    }
}

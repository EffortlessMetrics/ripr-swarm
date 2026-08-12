use super::{
    MANIFEST_PATH, RustJudgedPanelItem, load_and_validate_at, parse_json_without_duplicate_keys,
};
use crate::run::capture_output_in_dir_with_timeout;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCHEMA_VERSION: &str = "0.1";
const KIND: &str = "rust_judged_panel_replay_packet";
const PROCESS_TIMEOUT: Duration = Duration::from_mins(1);
const FIXED_GIT_DATE: &str = "2000-01-01T00:00:00Z";

#[derive(Serialize)]
struct ReplayPacket {
    schema_version: &'static str,
    kind: &'static str,
    case_id: String,
    expected_direction: String,
    seed_manifest_sha256: String,
    selected_row_sha256: String,
    diff_sha256: String,
    repository_fixture_id: String,
    base_sha: String,
    head_sha: String,
    tree_sha: String,
    source_file: &'static str,
    source_sha256: String,
    test_files: Vec<FileIdentity>,
    ripr_binary_path: &'static str,
    ripr_binary_sha256: String,
    ripr_version: String,
    build_profile: &'static str,
    enabled_features: Vec<String>,
    config_path: &'static str,
    config_sha256: String,
    analysis_profile: &'static str,
    argv: Vec<String>,
    analyzer_input_identity: String,
    execution: ExecutionRecord,
    raw_stdout_sha256: String,
    raw_stderr_sha256: String,
    raw_stdout_ref: String,
    raw_stderr_ref: String,
    selected_projection: Value,
    judgment: Value,
    runtime_calibration: Value,
    limitations: Vec<String>,
    non_claims: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    packet_digest: Option<String>,
}

#[derive(Serialize)]
struct FileIdentity {
    path: String,
    sha256: String,
}

#[derive(Serialize)]
struct ExecutionRecord {
    disposition: &'static str,
    exit_code: i32,
    timed_out: bool,
    analysis_complete: bool,
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
    materialize_repository(&case_root, &fixture)?;
    let base_sha = git(&case_root, &["rev-parse", "HEAD^"])?;
    let head_sha = git(&case_root, &["rev-parse", "HEAD"])?;
    let tree_sha = git(&case_root, &["rev-parse", "HEAD^{tree}"])?;
    let diff_source = root.join(&item.diff_path);
    let diff_bytes = fs::read(&diff_source)
        .map_err(|error| format!("read selected diff {}: {error}", diff_source.display()))?;
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
    )?;
    if output.timed_out {
        return Err(format!("replay case `{}` timed out", item.id));
    }
    let status = output
        .status
        .ok_or_else(|| format!("replay case `{}` returned no status", item.id))?;
    if !status.success() {
        return Err(format!(
            "replay case `{}` failed with {status}\nstdout:\n{}\nstderr:\n{}",
            item.id, output.stdout, output.stderr
        ));
    }
    let parsed: Value = serde_json::from_str(&output.stdout)
        .map_err(|error| format!("replay case `{}` emitted malformed JSON: {error}", item.id))?;
    let analysis_complete = parsed
        .pointer("/analysis_outcome/analysis_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !analysis_complete {
        return Err(format!(
            "replay case `{}` did not report complete analysis",
            item.id
        ));
    }
    let input_identity = parsed
        .pointer("/analysis_outcome/outcome/identity/input_identity")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("sha256:"))
        .ok_or_else(|| format!("replay case `{}` omitted analyzer input identity", item.id))?
        .to_string();
    let projection = select_projection(item, &parsed)?;
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
        schema_version: SCHEMA_VERSION,
        kind: KIND,
        case_id: item.id.clone(),
        expected_direction: item.expected_direction.clone(),
        seed_manifest_sha256: sha256(manifest_bytes),
        selected_row_sha256: sha256(&serde_json::to_vec(row).map_err(|error| error.to_string())?),
        diff_sha256: sha256(&diff_bytes),
        repository_fixture_id: item.repository.clone(),
        base_sha,
        head_sha,
        tree_sha,
        source_file: "src/lib.rs",
        source_sha256: sha256(&source),
        test_files: vec![FileIdentity {
            path: "tests/replay.rs".to_string(),
            sha256: sha256(&tests),
        }],
        ripr_binary_path: "<workspace-ripr-bin>",
        ripr_binary_sha256: binary_digest.to_string(),
        ripr_version: version.to_string(),
        build_profile: "supplied_workspace_binary",
        enabled_features: Vec::new(),
        config_path: "ripr.toml",
        config_sha256: sha256_file(&case_root.join("ripr.toml"))?,
        analysis_profile: "draft",
        argv: argv.clone(),
        analyzer_input_identity: input_identity,
        execution: ExecutionRecord {
            disposition: "complete",
            exit_code: status.code().unwrap_or(0),
            timed_out: false,
            analysis_complete,
        },
        raw_stdout_sha256: sha256(output.stdout.as_bytes()),
        raw_stderr_sha256: sha256(output.stderr.as_bytes()),
        raw_stdout_ref: format!("{}/stdout.json", item.id),
        raw_stderr_ref: format!("{}/stderr.txt", item.id),
        selected_projection: projection,
        judgment: json!({"status":"unjudged","labels":null}),
        runtime_calibration: json!({"status":"not_run","outcome":null,"evidence_ref":null}),
        limitations: parsed
            .pointer("/analysis_outcome/outcome/limitations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        non_claims: item.must_not_claim.clone(),
        packet_digest: None,
    };
    let identity_bytes = serde_json::to_vec(&packet).map_err(|error| error.to_string())?;
    packet.packet_digest = Some(sha256(&identity_bytes));
    let packet_bytes = serde_json::to_vec_pretty(&packet).map_err(|error| error.to_string())?;
    atomic_write(&case_out.join("packet.json"), &packet_bytes)
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

fn materialize_repository(root: &Path, fixture: &Fixture<'_>) -> Result<(), String> {
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
    fs::write(root.join("src/lib.rs"), fixture.new_source).map_err(|error| error.to_string())?;
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

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("publish {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

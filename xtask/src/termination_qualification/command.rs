//! `cargo xtask termination-qualification --binary <path>` (#4505).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::controls;
use super::corpus::{Case, ROW_DEADLINE, all_cases};
use super::exec::{self, Binary};
use super::model::{
    Aggregate, BinaryIdentity, CORPUS_SCHEMA, Control, Family, FamilyCount, HostIdentity, Packet,
    Row, RowResult, SCHEMA, render_json, render_markdown,
};
use crate::blind_journey::sha256_hex;
use crate::run as shared_run;

const DEFAULT_OUT: &str = "target/ripr/reports";
const DEFAULT_WORK: &str = "target/ripr/termination-qualification";
const REPORT_JSON: &str = "termination-qualification.json";
const REPORT_MD: &str = "termination-qualification.md";

pub(crate) struct Options {
    pub(crate) binary: PathBuf,
    pub(crate) out: PathBuf,
    pub(crate) work: PathBuf,
    pub(crate) expected_sha256: Option<String>,
    pub(crate) expected_version: Option<String>,
    pub(crate) families: Vec<Family>,
    pub(crate) removal_controls: Vec<(Family, PathBuf)>,
    pub(crate) repeat: bool,
    pub(crate) deadline: Duration,
    pub(crate) allow_incomplete: bool,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let packet = build_packet(&options)?;
    fs::create_dir_all(&options.out)
        .map_err(|error| format!("failed to create {}: {error}", options.out.display()))?;
    let json_path = options.out.join(REPORT_JSON);
    let md_path = options.out.join(REPORT_MD);
    fs::write(&json_path, render_json(&packet)?)
        .map_err(|error| format!("failed to write {}: {error}", json_path.display()))?;
    fs::write(&md_path, render_markdown(&packet))
        .map_err(|error| format!("failed to write {}: {error}", md_path.display()))?;
    println!(
        "termination-qualification: {} ({} rows, digest {}); wrote {} and {}",
        packet.aggregate.as_str(),
        packet.rows.len(),
        packet.digest,
        json_path.display(),
        md_path.display()
    );
    match packet.aggregate {
        Aggregate::Passed => Ok(()),
        Aggregate::Incomplete if options.allow_incomplete => Ok(()),
        _ => Err(format!(
            "termination-qualification {}: {}",
            packet.aggregate.as_str(),
            packet.aggregate_reasons.join("; ")
        )),
    }
}

pub(crate) fn build_packet(options: &Options) -> Result<Packet, String> {
    if !cfg!(unix) {
        // The child PATH, process-group custody and instrument stubs are Unix
        // contracts. A host that cannot honour them must not report a pass.
        return Err(
            "termination-qualification runs on a Unix host only; this host would skip every row"
                .to_string(),
        );
    }
    let identity = admit_binary(options)?;
    let binary = Binary {
        path: fs::canonicalize(&options.binary)
            .map_err(|error| format!("failed to resolve {}: {error}", options.binary.display()))?,
    };
    let run_root = options
        .work
        .join(format!("{}-{}", &identity.sha256[..12], std::process::id()));
    if run_root.exists() {
        return Err(format!(
            "owned work root {} already exists; refusing to reuse it",
            run_root.display()
        ));
    }
    fs::create_dir_all(&run_root)
        .map_err(|error| format!("failed to create {}: {error}", run_root.display()))?;
    // Children run with the case root as cwd, so every path handed to one
    // must be absolute.
    let run_root = fs::canonicalize(&run_root)
        .map_err(|error| format!("failed to resolve {}: {error}", run_root.display()))?;

    let cases: Vec<Case> = all_cases()
        .into_iter()
        .filter(|case| options.families.is_empty() || options.families.contains(&case.family))
        .collect();

    let mut rows = Vec::new();
    let mut observations = Vec::new();
    for case in &cases {
        let (row, observation) =
            exec::run_case(&binary, &run_root.join("pass1"), case, options.deadline);
        rows.push(row);
        observations.push(observation);
    }

    let mut controls_out: Vec<Control> = Vec::new();
    controls_out.extend(controls::repeat_control(
        &binary,
        &run_root.join("pass2"),
        &cases,
        &rows,
        options.repeat,
        options.deadline,
    ));
    controls_out.extend(controls::instrument_controls(
        &run_root.join("instrument"),
        &cases,
    ));
    controls_out.extend(controls::removal_controls(
        &run_root.join("removal"),
        &cases,
        &options.removal_controls,
        &identity.sha256,
        options.deadline,
    ));

    let work_cleanup = exec::cleanup(&run_root);
    let binary_path = identity.path.clone();
    let scrub = |text: &mut String| {
        *text = text
            .replace(run_root.to_string_lossy().as_ref(), "<work-root>")
            .replace(binary_path.as_str(), "<binary>");
    };
    for row in &mut rows {
        scrub(&mut row.detail);
    }
    for control in &mut controls_out {
        scrub(&mut control.detail);
        if let Some(observed) = &mut control.observed {
            scrub(observed);
        }
    }
    let families = family_counts(&rows, &cases);
    let (aggregate, aggregate_reasons) =
        aggregate(&rows, &families, &controls_out, options, &work_cleanup);
    let mut packet = Packet {
        schema: SCHEMA,
        corpus_schema: CORPUS_SCHEMA,
        binary: identity,
        host: HostIdentity {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            family: std::env::consts::FAMILY.to_string(),
        },
        environment_boundary: exec::environment_boundary(),
        families,
        rows,
        controls: controls_out,
        aggregate,
        aggregate_reasons,
        non_claims: non_claims(),
        digest: String::new(),
        observations,
    };
    packet.digest = packet_digest(&packet)?;
    Ok(packet)
}

fn admit_binary(options: &Options) -> Result<BinaryIdentity, String> {
    let path = fs::canonicalize(&options.binary).map_err(|error| {
        format!(
            "--binary {} is not readable: {error}",
            options.binary.display()
        )
    })?;
    if !path.is_file() {
        return Err(format!("--binary {} is not a file", path.display()));
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let sha256 = sha256_hex(&bytes);
    if let Some(expected) = &options.expected_sha256
        && !expected.eq_ignore_ascii_case(&sha256)
    {
        return Err(format!(
            "binary sha256 {sha256} does not match --expected-sha256 {expected}"
        ));
    }
    let version_args = vec!["--version".to_string()];
    let version = shared_run::capture_bytes_in_dir_with_timeout(
        &path,
        &version_args,
        path.parent().unwrap_or_else(|| Path::new(".")),
        &[("PATH", exec::CHILD_PATH)],
        &[],
        Duration::from_secs(30),
        "termination binary --version",
    )?;
    if version.timed_out || !version.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "`{} --version` did not exit cleanly; this is not a runnable ripr binary",
            path.display()
        ));
    }
    let reported_version = String::from_utf8_lossy(&version.stdout).trim().to_string();
    if let Some(expected) = &options.expected_version
        && !reported_version.contains(expected.as_str())
    {
        return Err(format!(
            "binary reports `{reported_version}`, expected a version containing `{expected}`"
        ));
    }
    Ok(BinaryIdentity {
        path: path.to_string_lossy().to_string(),
        bytes: bytes.len() as u64,
        sha256,
        reported_version,
        path_resolution: format!(
            "launched by absolute path; child PATH is `{}`, so no ambient `ripr` can resolve",
            exec::CHILD_PATH
        ),
    })
}

pub(super) fn family_counts(rows: &[Row], cases: &[Case]) -> Vec<FamilyCount> {
    Family::ALL
        .iter()
        .map(|family| {
            let selected = cases.iter().filter(|case| case.family == *family).count();
            let family_rows = rows.iter().filter(|row| row.family == *family);
            let executed = family_rows
                .clone()
                .filter(|row| {
                    !matches!(row.result, RowResult::NotRun | RowResult::InstrumentFailure)
                })
                .count();
            let passed = family_rows.filter(|row| row.result.is_pass()).count();
            FamilyCount {
                family: *family,
                selected,
                executed,
                passed,
            }
        })
        .collect()
}

pub(super) fn aggregate(
    rows: &[Row],
    families: &[FamilyCount],
    controls: &[Control],
    options: &Options,
    work_cleanup: &str,
) -> (Aggregate, Vec<String>) {
    let mut failures = Vec::new();
    let mut incomplete = Vec::new();
    for count in families {
        if count.selected == 0 {
            if options.families.is_empty() {
                failures.push(format!(
                    "family {} has no selected rows",
                    count.family.as_str()
                ));
            } else if options.families.contains(&count.family) {
                failures.push(format!(
                    "family {} selected zero rows",
                    count.family.as_str()
                ));
            } else {
                incomplete.push(format!(
                    "family {} was excluded by --family",
                    count.family.as_str()
                ));
            }
        } else if count.executed == 0 {
            failures.push(format!(
                "family {} executed zero rows",
                count.family.as_str()
            ));
        }
    }
    for row in rows {
        if !row.result.is_pass() {
            failures.push(format!(
                "{}: {} ({})",
                row.id,
                row.result.as_str(),
                row.detail
            ));
        }
    }
    for control in controls {
        if control.observed.is_none() {
            incomplete.push(format!("{}: {}", control.id, control.detail));
        } else if !control.discriminated {
            failures.push(format!(
                "control {} did not discriminate: {}",
                control.id, control.detail
            ));
        }
    }
    if work_cleanup != "removed" {
        failures.push(format!("owned work root cleanup: {work_cleanup}"));
    }
    if rows.is_empty() {
        failures.push("zero rows were selected".to_string());
    }
    if !failures.is_empty() {
        failures.extend(incomplete);
        (Aggregate::Failed, failures)
    } else if !incomplete.is_empty() {
        (Aggregate::Incomplete, incomplete)
    } else {
        (Aggregate::Passed, Vec::new())
    }
}

fn non_claims() -> Vec<String> {
    vec![
        "terminates honestly on this retained corpus only; not absence of every crash or hang class".to_string(),
        "no universal parser-depth or recursion proof, and no fuzzing".to_string(),
        "Python: f-string replacement-field shapes and any other construct the tree-depth estimate does not model are outside this corpus".to_string(),
        "wall time is an instrument observation; it is not part of the packet digest and proves no latency".to_string(),
        "the packet does not bind the binary to a source SHA, tree, candidate ref or package; those identities are supplied by the candidate lane".to_string(),
        "LSP routes are not exercised here; this packet covers `ripr check` only".to_string(),
        "no orphan scan beyond the shared process owner's tree termination and the instrument cancellation control".to_string(),
        "does not decide qualification; #2769 consumes the packet".to_string(),
    ]
}

/// SHA-256 over the semantic packet: rows, controls, binary, host and
/// aggregate. Observations (elapsed time) and the digest field are excluded.
pub(super) fn packet_digest(packet: &Packet) -> Result<String, String> {
    let mut value = serde_json::to_value(packet)
        .map_err(|error| format!("failed to serialize termination packet: {error}"))?;
    if let Some(object) = value.as_object_mut() {
        object.remove("observations");
        object.remove("digest");
        // The binary path is a host-local location, not a semantic identity.
        if let Some(binary) = object
            .get_mut("binary")
            .and_then(|value| value.as_object_mut())
        {
            binary.remove("path");
        }
    }
    let text = serde_json::to_string(&value)
        .map_err(|error| format!("failed to render termination packet identity: {error}"))?;
    let mut digest = Sha256::new();
    digest.update(text.as_bytes());
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut binary = None;
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut work = PathBuf::from(DEFAULT_WORK);
    let mut expected_sha256 = None;
    let mut expected_version = None;
    let mut families = Vec::new();
    let mut removal_controls = Vec::new();
    let mut repeat = true;
    let mut deadline = ROW_DEADLINE;
    let mut allow_incomplete = false;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--no-repeat" => {
                repeat = false;
                index += 1;
                continue;
            }
            "--allow-incomplete" => {
                allow_incomplete = true;
                index += 1;
                continue;
            }
            _ => {}
        }
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value; {}", usage()))?
            .clone();
        match flag {
            "--binary" => binary = Some(PathBuf::from(value)),
            "--out" => out = PathBuf::from(value),
            "--work-dir" => work = PathBuf::from(value),
            "--expected-sha256" => expected_sha256 = Some(value),
            "--expected-version" => expected_version = Some(value),
            "--family" => families.push(parse_family(&value)?),
            "--removal-control" => {
                let (family, path) = value.split_once('=').ok_or_else(|| {
                    format!("--removal-control expects <family>=<binary>; {}", usage())
                })?;
                removal_controls.push((parse_family(family)?, PathBuf::from(path)));
            }
            "--deadline-seconds" => {
                let seconds: u64 = value.parse().map_err(|error| {
                    format!("--deadline-seconds must be an integer, got `{value}` ({error})")
                })?;
                if seconds == 0 {
                    return Err("--deadline-seconds must be at least 1".to_string());
                }
                deadline = Duration::from_secs(seconds);
            }
            other => {
                return Err(format!(
                    "unknown termination-qualification argument `{other}`; {}",
                    usage()
                ));
            }
        }
        index += 2;
    }
    let Some(binary) = binary else {
        return Err(usage().to_string());
    };
    Ok(Options {
        binary,
        out,
        work,
        expected_sha256,
        expected_version,
        families,
        removal_controls,
        repeat,
        deadline,
        allow_incomplete,
    })
}

fn parse_family(value: &str) -> Result<Family, String> {
    Family::ALL
        .iter()
        .copied()
        .find(|family| family.as_str() == value)
        .ok_or_else(|| {
            format!(
                "unknown family `{value}`; expected one of {}",
                Family::ALL
                    .iter()
                    .map(|family| family.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn usage() -> &'static str {
    "usage: cargo xtask termination-qualification --binary <path> [--expected-sha256 <hex>] [--expected-version <text>] [--family <name>]... [--removal-control <family>=<binary>]... [--no-repeat] [--deadline-seconds <n>] [--allow-incomplete] [--work-dir <dir>] [--out <dir>]"
}

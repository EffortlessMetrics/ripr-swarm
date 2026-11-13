//! Native compiled inventory and exact result attribution for capture and
//! offline replay integration. Source discovery remains a producer operation.
use super::native::{ObservationStatus, Subject, UnregisteredResult, sha256 as hash};
use super::{self as formal, kani_output as lane};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeList {
    #[serde(rename = "kani-version")]
    kani_version: String,
    #[serde(rename = "file-version")]
    file_version: String,
    #[serde(rename = "standard-harnesses")]
    standard: BTreeMap<String, Vec<String>>,
    #[serde(rename = "contract-harnesses")]
    contracts_harnesses: BTreeMap<String, Vec<String>>,
    contracts: Vec<serde_json::Value>,
    totals: NativeTotals,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeTotals {
    #[serde(rename = "standard-harnesses")]
    standard: usize,
    #[serde(rename = "contract-harnesses")]
    contracts: usize,
    #[serde(rename = "functions-under-contract")]
    functions: usize,
}
pub fn native_inventory(
    bytes: &[u8],
    package: &str,
    root: &Path,
    sources: &BTreeMap<String, String>,
) -> Result<Vec<Subject>, String> {
    let list: NativeList = formal::parse_json(bytes)?;
    if list.kani_version != "0.68.0" || list.file_version != "0.1" {
        return Err("unsupported Kani native inventory version".to_string());
    }
    if !list.contracts_harnesses.is_empty()
        || !list.contracts.is_empty()
        || list.totals.contracts != 0
        || list.totals.functions != 0
    {
        return Err("contract proof inventory has no admitted ordinary-harness route".to_string());
    }
    let mut subjects = Vec::new();
    let mut identities = BTreeSet::new();
    for (file, names) in list.standard {
        let path = Path::new(&file);
        let path = if path.is_absolute() {
            path.strip_prefix(root).map_err(|error| {
                format!("native harness source is outside captured repository: {error}")
            })?
        } else {
            path
        };
        let file = path.to_string_lossy().replace('\\', "/");
        let digest = sources
            .get(&file)
            .ok_or("native harness file is outside captured source closure")?;
        for qualified in names {
            if qualified.is_empty() || !identities.insert(qualified.clone()) {
                return Err("missing or duplicate qualified native harness identity".to_string());
            }
            let name = qualified
                .rsplit("::")
                .next()
                .ok_or("native harness has no name")?
                .to_string();
            subjects.push(Subject {
                package: package.to_string(),
                file: file.clone(),
                name,
                qualified_name: Some(qualified),
                source_digest: digest.clone(),
            });
        }
    }
    if list.totals.standard != subjects.len() {
        return Err("native inventory count contradicts its unique subjects".to_string());
    }
    subjects.sort();
    Ok(subjects)
}

pub fn compare_inventories(declared: &[Subject], native: &[Subject]) -> Vec<String> {
    let key = |s: &Subject| (s.package.clone(), s.file.clone(), s.name.clone());
    let mut expected = BTreeMap::<_, usize>::new();
    let mut observed = BTreeMap::<_, usize>::new();
    for subject in declared {
        *expected.entry(key(subject)).or_default() += 1;
    }
    for subject in native {
        *observed.entry(key(subject)).or_default() += 1;
    }
    let mut reasons = Vec::new();
    for (name, count) in &expected {
        if *count != 1 || observed.get(name) != Some(&1) {
            reasons.push(format!(
                "declared/native subject is missing or ambiguous: {}:{}::{}",
                name.0, name.1, name.2
            ));
        }
    }
    for name in observed.keys() {
        if !expected.contains_key(name) {
            reasons.push(format!(
                "unexpected native subject: {}:{}::{}",
                name.0, name.1, name.2
            ));
        }
    }
    reasons
}

pub fn observe(
    subjects: &[Subject],
    stdout: &str,
) -> (Vec<UnregisteredResult>, Vec<String>, Vec<String>) {
    let blocks = lane::split_harness_blocks(stdout);
    let mut rows = Vec::new();
    let mut observed = Vec::new();
    let mut reasons = Vec::new();
    let expected: BTreeMap<_, _> = subjects
        .iter()
        .filter_map(|s| s.qualified_name.as_deref().map(|n| (n, s)))
        .collect();
    for (name, _) in &blocks {
        if let Some(subject) = expected.get(name.as_str()) {
            observed.push(subject.id());
        } else {
            reasons.push(format!("unexpected native result block: {name}"));
        }
    }
    for subject in subjects {
        let matches: Vec<_> = blocks
            .iter()
            .filter(|(name, _)| subject.qualified_name.as_ref() == Some(name))
            .collect();
        let (status, detail, time, covers, block_digest) = match matches.as_slice() {
            [(name, block)] => {
                let harness = lane::Harness {
                    file: subject.file.clone(),
                    name: name.clone(),
                };
                let mapped = lane::results_from_output(&[harness], stdout);
                let result = mapped.first();
                let (status, detail, time) = result
                    .map(|r| {
                        (
                            observation_status(&r.status),
                            r.status.detail().to_string(),
                            r.time.clone(),
                        )
                    })
                    .unwrap_or((
                        ObservationStatus::NotEstablished,
                        "native result was not attributable".to_string(),
                        "n/a".to_string(),
                    ));
                (
                    status,
                    detail,
                    time,
                    lane::cover_observation(block),
                    Some(hash(block.as_bytes())),
                )
            }
            [] => (
                ObservationStatus::NotEstablished,
                "kani printed no result block for this harness".to_string(),
                "n/a".to_string(),
                None,
                None,
            ),
            _ => (
                ObservationStatus::NotEstablished,
                "kani printed duplicate native result blocks".to_string(),
                "n/a".to_string(),
                None,
                None,
            ),
        };
        let parsed_properties = match matches.as_slice() {
            [(_, block)] => lane::native_properties(block).and_then(|properties| {
                lane::cover_property_projection(&properties).map(|covers| (properties, covers))
            }),
            _ => Ok((Vec::new(), Vec::new())),
        };
        let (mut status, mut detail) = (status, detail);
        let (properties, cover_properties) = match parsed_properties {
            Ok(value) => value,
            Err(error) => {
                status = ObservationStatus::NotEstablished;
                detail = error;
                (Vec::new(), Vec::new())
            }
        };
        if status == ObservationStatus::Verified
            && !covers.as_ref().is_some_and(|covers| {
                covers.total as usize == cover_properties.len()
                    && cover_properties.iter().all(|p| p.status == "SATISFIED")
            })
        {
            status = ObservationStatus::NotEstablished;
            detail = "native cover properties do not establish the complete positive summary"
                .to_string();
        }
        let native_verdict = match matches.as_slice() {
            [(_, block)] => lane::native_verdict(block),
            [] => formal::Verdict::Missing,
            _ => formal::Verdict::Ambiguous,
        };
        if (status == ObservationStatus::Verified && native_verdict != formal::Verdict::Verified)
            || (status == ObservationStatus::Failed
                && !matches!(
                    native_verdict,
                    formal::Verdict::Counterexample | formal::Verdict::UnreachableCover
                ))
        {
            status = ObservationStatus::NotEstablished;
            detail = "native verdict does not establish the ordinary harness result".to_string();
        }
        rows.push(UnregisteredResult {
            subject: subject.clone(),
            status,
            native_verdict,
            detail,
            kani_time: time,
            duration_ms: match matches.as_slice() {
                [(_, block)] => lane::verification_duration_ms(block),
                _ => None,
            },
            covers,
            cover_properties,
            properties,
            raw_block_digest: block_digest,
        });
    }
    observed.sort();
    if observed.windows(2).any(|w| w[0] == w[1]) {
        reasons.push("duplicate observed native subjects".to_string());
        observed.dedup();
    }
    let selected: BTreeSet<_> = subjects.iter().map(Subject::id).collect();
    let observed_set: BTreeSet<_> = observed.iter().cloned().collect();
    for missing in selected.difference(&observed_set) {
        reasons.push(format!("missing native result: {missing}"));
    }
    (rows, observed, reasons)
}
fn observation_status(status: &lane::HarnessStatus) -> ObservationStatus {
    match status {
        lane::HarnessStatus::Verified => ObservationStatus::Verified,
        lane::HarnessStatus::Failed(_) => ObservationStatus::Failed,
        lane::HarnessStatus::NotEstablished(_) => ObservationStatus::NotEstablished,
    }
}

/// Interpret the exact captured streams under their observed process outcome.
/// Shared seam for later live collection and passive replay integration.
/// Valid-looking stdout cannot erase a timeout, incomplete stream, unsupported
/// stderr result, or unsupported tool header. Available stdout remains diagnostic
/// evidence even when the overall capture cannot establish a result.
pub fn observe_invocation(
    subjects: &[Subject],
    invocation: &super::native::Invocation,
    stdout: Option<&[u8]>,
    stderr: Option<&[u8]>,
) -> (Vec<UnregisteredResult>, Vec<String>, Vec<String>) {
    let text = stdout.and_then(|out| std::str::from_utf8(out).ok());
    let errors = stderr.and_then(|err| std::str::from_utf8(err).ok());
    let capture_problem = !invocation.capture_complete || text.is_none() || errors.is_none();
    let unsupported_stderr = errors.is_some_and(|errors| {
        errors
            .lines()
            .any(|line| line.starts_with("Checking harness "))
    });
    let (mut rows, observed, mut reasons) = observe(subjects, text.unwrap_or(""));
    if unsupported_stderr {
        reasons.push("result headers on unsupported stderr stream".to_string());
    }
    if capture_problem {
        reasons.push("native streams were not completely captured as UTF-8".to_string());
    }
    let header_error = text.and_then(|text| super::native::native_tool_header(text).err());
    for row in &mut rows {
        let problem = if invocation.timed_out {
            if row.raw_block_digest.is_none() {
                row.native_verdict = formal::Verdict::Timeout;
            }
            Some("native verification exceeded its owned process deadline".to_string())
        } else if invocation.exit_code.is_none() || invocation.error.is_some() {
            if row.raw_block_digest.is_none() {
                row.native_verdict = formal::Verdict::AbnormalProcess;
            }
            Some("native verification did not have a complete normal process outcome".to_string())
        } else if capture_problem {
            Some("native streams were not completely captured as UTF-8".to_string())
        } else if unsupported_stderr {
            Some("result headers on unsupported stderr stream".to_string())
        } else if let Some(error) = &header_error {
            Some(error.clone())
        } else if invocation.exit_code != Some(0) && row.status == ObservationStatus::Verified {
            Some("successful result text accompanied a nonzero native package exit".to_string())
        } else {
            None
        };
        if let Some(problem) = problem {
            row.status = ObservationStatus::NotEstablished;
            row.detail = problem;
        }
    }
    if let Some(error) = header_error {
        reasons.push(error);
    }
    (rows, observed, reasons)
}

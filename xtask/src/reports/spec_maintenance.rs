//! Advisory, deterministic inventory of specification documents.
//!
//! This report is deliberately separate from `check-spec-format`: it explains
//! why a maintainer may want to look at a document, but never declares a
//! document invalid or changes lifecycle/support state.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const JSON_FILE: &str = "spec-maintenance.json";
const MARKDOWN_FILE: &str = "spec-maintenance.md";
const SCHEMA_VERSION: &str = "1";
const USAGE: &str = "usage: cargo xtask specs maintenance --as-of YYYY-MM-DD [--json]";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct SpecMaintenanceReportV1 {
    pub(crate) schema_version: String,
    pub(crate) producer: String,
    pub(crate) as_of: String,
    pub(crate) history: HistoryObservation,
    pub(crate) denominator: Denominator,
    pub(crate) status_counts: BTreeMap<String, usize>,
    pub(crate) specs: Vec<SpecMaintenanceRow>,
    pub(crate) reason_counts: BTreeMap<String, usize>,
    pub(crate) limitations: Vec<String>,
    pub(crate) claim_boundary: ClaimBoundary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct HistoryObservation {
    pub(crate) available: bool,
    pub(crate) source: String,
    pub(crate) limitation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Denominator {
    pub(crate) discoverable: usize,
    pub(crate) included: usize,
    pub(crate) omitted: Vec<OmittedInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct OmittedInput {
    pub(crate) path: String,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct SpecMaintenanceRow {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) content_digest: String,
    pub(crate) status: String,
    pub(crate) reason_codes: Vec<String>,
    pub(crate) evidence_refs: Vec<String>,
    pub(crate) next_route: String,
    pub(crate) limitations: Vec<String>,
    pub(crate) age_observation: Option<AgeObservation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AgeObservation {
    pub(crate) last_changed: String,
    pub(crate) bucket: String,
    pub(crate) days: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ClaimBoundary {
    pub(crate) establishes: String,
    pub(crate) does_not_establish: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Options {
    as_of: String,
    json: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryInput {
    repository_available: bool,
    source: String,
    limitation: Option<String>,
    last_changed: BTreeMap<String, String>,
}

impl Default for HistoryInput {
    fn default() -> Self {
        Self {
            repository_available: false,
            source: "none".to_string(),
            limitation: Some(
                "Git/history input was unavailable; repository-only findings remain available"
                    .to_string(),
            ),
            last_changed: BTreeMap::new(),
        }
    }
}

pub(crate) fn spec_maintenance(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_options(args)?;
    let root = Path::new(".");
    let history = capture_history(root)?;
    let report = build_report(root, &options.as_of, &history)?;
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("serialize spec maintenance report: {error}"))?;
    crate::write_report(JSON_FILE, &format!("{json}\n"))?;
    crate::write_report(MARKDOWN_FILE, &render_markdown(&report))?;
    if options.json {
        println!("{json}");
    } else {
        println!("Wrote target/ripr/reports/{JSON_FILE}");
        println!("Wrote target/ripr/reports/{MARKDOWN_FILE}");
    }
    Ok(())
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut as_of = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "maintenance" => {}
            "--json" => json = true,
            "--as-of" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing value for --as-of\n".to_string() + USAGE)?;
                validate_date(value)?;
                as_of = Some(value.clone());
            }
            other => {
                return Err(format!(
                    "unknown specs maintenance argument `{other}`\n{USAGE}"
                ));
            }
        }
        index += 1;
    }
    let as_of = as_of.ok_or_else(|| format!("missing required --as-of\n{USAGE}"))?;
    Ok(Options { as_of, json })
}

fn validate_date(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(format!(
            "--as-of must be YYYY-MM-DD, got `{value}`\n{USAGE}"
        ));
    }
    let year = value[0..4]
        .parse::<i32>()
        .map_err(|error| format!("invalid --as-of year: {error}"))?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|error| format!("invalid --as-of month: {error}"))?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|error| format!("invalid --as-of day: {error}"))?;
    if year < 1
        || !(1..=12).contains(&month)
        || day == 0
        || i64::from(day) > days_in_month(i64::from(year), i64::from(month))
    {
        return Err(format!("--as-of is not a valid calendar date: `{value}`"));
    }
    Ok(())
}

pub(crate) fn build_report(
    root: &Path,
    as_of: &str,
    history: &HistoryInput,
) -> Result<SpecMaintenanceReportV1, String> {
    validate_date(as_of)?;
    let spec_root = root.join("docs/specs");
    let files = collect_markdown_files(&spec_root)?;
    let mut included = Vec::new();
    let mut omitted = Vec::new();
    for path in files {
        let relative = relative_path(root, &path);
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if let Some(id) = parse_spec_id(name) {
            let contents = fs::read(&path)
                .map_err(|error| format!("read required spec {relative}: {error}"))?;
            let text = String::from_utf8(contents.clone())
                .map_err(|error| format!("required spec {relative} is not UTF-8: {error}"))?;
            included.push(build_row(
                root, &id, &relative, &contents, &text, as_of, history,
            ));
        } else {
            omitted.push(OmittedInput {
                path: relative,
                reason: "unsupported-or-non-discoverable-markdown-input".to_string(),
            });
        }
    }
    // The spec index is a discoverable denominator input even when absent:
    // an empty or index-less specs directory still owes the reader an account
    // of what was not scannable, per the denominator-honesty contract.
    let index_path = spec_root.join("README.md");
    if !index_path.exists() {
        omitted.push(OmittedInput {
            path: relative_path(root, &index_path),
            reason: "spec-index-missing".to_string(),
        });
    }
    included.sort_by(|left, right| left.id.cmp(&right.id).then(left.path.cmp(&right.path)));
    omitted.sort_by(|left, right| left.path.cmp(&right.path));
    let mut reason_counts = BTreeMap::new();
    let mut status_counts = BTreeMap::new();
    for row in &included {
        *status_counts.entry(row.status.clone()).or_insert(0) += 1;
        for reason in &row.reason_codes {
            *reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
    }
    let history = HistoryObservation {
        available: history.repository_available,
        source: history.source.clone(),
        limitation: history.limitation.clone(),
    };
    let mut limitations = vec![
        "age is an observation and ordering hint, not a validity or lifecycle decision".to_string(),
    ];
    if let Some(limitation) = &history.limitation {
        limitations.push(limitation.clone());
    }
    Ok(SpecMaintenanceReportV1 {
        schema_version: SCHEMA_VERSION.to_string(),
        producer: "xtask specs maintenance".to_string(),
        as_of: as_of.to_string(),
        history,
        // A present file counts toward the discoverable denominator; the
        // synthetic missing-index record documents an absent input and must
        // not inflate the count of documents the scan saw.
        denominator: Denominator {
            discoverable: included.len()
                + omitted
                    .iter()
                    .filter(|input| input.reason != "spec-index-missing")
                    .count(),
            included: included.len(),
            omitted,
        },
        status_counts,
        specs: included,
        reason_counts,
        limitations,
        claim_boundary: ClaimBoundary {
            establishes: "An explainable advisory queue of specification documents deserving maintainer attention".to_string(),
            does_not_establish: vec![
                "specification correctness, implementation, evidence, or support".to_string(),
                "acceptance, deprecation, supersession, migration, merge eligibility, or branch protection".to_string(),
            ],
        },
    })
}

fn collect_markdown_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let entries =
        fs::read_dir(root).map_err(|error| format!("read {}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read spec directory entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_markdown_files(&path)?);
        } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(files)
}

fn capture_history(root: &Path) -> Result<HistoryInput, String> {
    let inside =
        match crate::run::run_output_optional("git", &["rev-parse", "--is-inside-work-tree"]) {
            Ok(output) => output,
            Err(_) => return Ok(HistoryInput::default()),
        };
    let repository_available = inside.trim() == "true";
    if !repository_available {
        return Ok(HistoryInput {
            repository_available: false,
            source: "none".to_string(),
            limitation: Some(
                "Git/history input was unavailable; repository-only findings remain available"
                    .to_string(),
            ),
            last_changed: BTreeMap::new(),
        });
    }
    let mut last_changed = BTreeMap::new();
    for path in collect_markdown_files(&root.join("docs/specs"))? {
        let relative = relative_path(root, &path);
        let changed = crate::run::run_output_optional(
            "git",
            &["log", "-1", "--format=%cI", "--", relative.as_str()],
        )?;
        if let Some(date) = changed
            .lines()
            .next()
            .filter(|date| !date.trim().is_empty())
        {
            last_changed.insert(relative, date.trim().chars().take(10).collect());
        }
    }
    Ok(HistoryInput {
        repository_available: true,
        source: "captured-git-observation".to_string(),
        limitation: None,
        last_changed,
    })
}

fn age_bucket(days: i64) -> String {
    match days {
        ..=0 => "same-day-or-future".to_string(),
        1..=89 => "under-90-days".to_string(),
        90..=364 => "90-to-364-days".to_string(),
        _ => "365-days-or-more".to_string(),
    }
}

fn days_between(start: &str, end: &str) -> Option<i64> {
    let start = parse_date_parts(start)?;
    let end = parse_date_parts(end)?;
    Some(civil_days(end) - civil_days(start))
}

fn parse_date_parts(value: &str) -> Option<(i64, i64, i64)> {
    if value.len() != 10
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
    {
        return None;
    }
    let year = value[0..4].parse().ok()?;
    let month = value[5..7].parse().ok()?;
    let day = value[8..10].parse().ok()?;
    if month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn civil_days((year, month, day): (i64, i64, i64)) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = (if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    }) / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_index = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era
}

fn parse_spec_id(name: &str) -> Option<String> {
    // The repository's canonical naming is `RIPR-SPEC-NNNN-slug.md`; reuse the
    // shared identifier parser so this report and the spec gates agree on what
    // a spec file is.
    crate::spec_id_from_file_name(name)
}

fn build_row(
    root: &Path,
    id: &str,
    path: &str,
    bytes: &[u8],
    text: &str,
    as_of: &str,
    history: &HistoryInput,
) -> SpecMaintenanceRow {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = format!("sha256:{:x}", hasher.finalize());
    let status = text
        .lines()
        .find_map(|line| line.strip_prefix("Status:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string();
    let mut reasons = Vec::new();
    let mut evidence = vec![path.to_string()];
    let mut limitations = Vec::new();
    // Reasons are derived from document structure — headings — rather than
    // bare token presence anywhere in the text (token coincidence clears a
    // spec for an unrelated mention and flags one for a link target).
    let headings: Vec<&str> = text.lines().filter(|line| line.starts_with('#')).collect();
    let has_review_heading = headings
        .iter()
        .any(|heading| heading.to_ascii_lowercase().contains("review"));
    if !has_review_heading {
        reasons.push("never_reviewed".to_string());
    }
    let has_test_mapping_heading = headings
        .iter()
        .any(|heading| heading.trim().eq_ignore_ascii_case("## test mapping"));
    if status.eq_ignore_ascii_case("accepted") && !has_test_mapping_heading {
        reasons.push("accepted_without_current_or_planned_test_mapping".to_string());
    }
    for link in markdown_links(text) {
        let target = root
            .join(path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&link);
        if !target.exists() {
            reasons.push("linked_artifact_missing".to_string());
            evidence.push(format!("missing:{link}"));
        }
    }
    let last_changed = history.last_changed.get(path).cloned();
    if !history.repository_available || last_changed.is_none() {
        reasons.push("history_unavailable".to_string());
        limitations.push(if history.repository_available {
            "no Git history was available for this spec".to_string()
        } else {
            "Git/history input was unavailable; repository-only findings remain available"
                .to_string()
        });
    }
    let age_observation = last_changed.and_then(|last_changed| {
        let days = days_between(last_changed.as_str(), as_of)?;
        let bucket = age_bucket(days);
        if days >= 365 {
            reasons.push("periodic_attention_due".to_string());
        }
        Some(AgeObservation {
            last_changed,
            bucket,
            days: Some(days),
        })
    });
    if reasons.is_empty() {
        reasons.push("no_objective_maintenance_finding".to_string());
    }
    reasons.sort();
    reasons.dedup();
    evidence.sort();
    evidence.dedup();
    SpecMaintenanceRow {
        id: id.to_string(),
        path: path.to_string(),
        content_digest: digest,
        status,
        reason_codes: reasons,
        evidence_refs: evidence,
        next_route: "maintainer-review/spec-maintenance".to_string(),
        limitations,
        age_observation,
    }
}

fn markdown_links(text: &str) -> Vec<String> {
    text.split("](")
        .skip(1)
        .filter_map(|part| part.split(')').next())
        .filter(|link| !link.starts_with('#') && !link.contains("://"))
        // A link target may carry a title (`path "Title"`) and a fragment
        // (`path#section`); existence is judged on the bare path only.
        .map(|link| {
            let bare = link.split_whitespace().next().unwrap_or_default();
            bare.split('#').next().unwrap_or_default().to_string()
        })
        .filter(|link| !link.is_empty())
        .collect()
}

fn relative_path(root: &Path, path: &Path) -> String {
    crate::normalize_path(path.strip_prefix(root).unwrap_or(path))
}

fn render_markdown(report: &SpecMaintenanceReportV1) -> String {
    let mut out = String::from("# Specification maintenance inventory\n\nStatus: advisory\n\n");
    out.push_str(&format!(
        "- Schema: `{}`\n- As of: `{}`\n- Discoverable: `{}`\n- Included: `{}`\n",
        report.schema_version,
        report.as_of,
        report.denominator.discoverable,
        report.denominator.included
    ));
    out.push_str(&format!(
        "- History: `{}`\n\n",
        if report.history.available {
            "available"
        } else {
            "unavailable"
        }
    ));
    out.push_str("## Candidates\n\n| ID | Path | Status | Reasons | Next route |\n| --- | --- | --- | --- | --- |\n");
    for row in &report.specs {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` |\n",
            row.id,
            row.path,
            row.status,
            row.reason_codes.join(", "),
            row.next_route
        ));
    }
    if report.specs.is_empty() {
        out.push_str("| — | — | — | none | — |\n");
    }
    out.push_str("\n## Omitted inputs\n\n");
    if report.denominator.omitted.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for input in &report.denominator.omitted {
            out.push_str(&format!("- `{}` — {}\n", input.path, input.reason));
        }
        out.push('\n');
    }
    out.push_str("## Limitations and claim boundary\n\n");
    for limitation in &report.limitations {
        out.push_str(&format!("- {}\n", limitation));
    }
    out.push_str(&format!(
        "\nEstablishes: {}.\n\nDoes not establish:\n",
        report.claim_boundary.establishes
    ));
    for claim in &report.claim_boundary.does_not_establish {
        out.push_str(&format!("- {}\n", claim));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root() -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-spec-maintenance-{stamp}"));
        fs::create_dir_all(root.join("docs/specs")).map_err(|error| error.to_string())?;
        Ok(root)
    }

    #[test]
    fn report_is_stable_and_history_limitation_is_visible() -> Result<(), String> {
        let root = fixture_root()?;
        let path = root.join("docs/specs/RIPR-SPEC-9999-example.md");
        fs::write(&path, "# Example\n\nStatus: proposed\n").map_err(|error| error.to_string())?;
        fs::write(root.join("docs/specs/README.md"), "index").map_err(|error| error.to_string())?;
        let left = build_report(&root, "2026-08-27", &HistoryInput::default())?;
        let right = build_report(&root, "2026-08-27", &HistoryInput::default())?;
        if left != right {
            return Err("fixed inputs did not produce stable report".to_string());
        }
        if !left.reason_counts.contains_key("history_unavailable") {
            return Err("history limitation was not reported".to_string());
        }
        if left.denominator.omitted.len() != 1 {
            return Err("unsupported input was not reported".to_string());
        }
        Ok(())
    }

    #[test]
    fn age_is_an_observation_and_periodic_reason_has_a_basis() -> Result<(), String> {
        let root = fixture_root()?;
        fs::write(
            root.join("docs/specs/RIPR-SPEC-9999-example.md"),
            "# Example\n\nStatus: proposed\n",
        )
        .map_err(|error| error.to_string())?;
        let mut history = HistoryInput {
            repository_available: true,
            source: "fixture".to_string(),
            ..HistoryInput::default()
        };
        history.last_changed.insert(
            "docs/specs/RIPR-SPEC-9999-example.md".to_string(),
            "2025-01-01".to_string(),
        );
        let report = build_report(&root, "2026-08-27", &history)?;
        let row = report
            .specs
            .first()
            .ok_or_else(|| "missing fixture row".to_string())?;
        if row.age_observation.as_ref().map(|age| age.bucket.as_str()) != Some("365-days-or-more") {
            return Err("age bucket was not observed deterministically".to_string());
        }
        if !row
            .reason_codes
            .contains(&"periodic_attention_due".to_string())
        {
            return Err("periodic reason lacked its age basis".to_string());
        }
        if row
            .reason_codes
            .contains(&"history_unavailable".to_string())
        {
            return Err("available history was reported as unavailable".to_string());
        }
        Ok(())
    }

    #[test]
    fn periodic_attention_threshold_is_pinned_at_365_days() -> Result<(), String> {
        let build = |last_changed: &str| -> Result<(bool,), String> {
            let root = fixture_root()?;
            let relative = "docs/specs/RIPR-SPEC-9999-example.md";
            fs::write(
                root.join(&relative),
                format!(
                    "# Example

## Review
2026-01-02 reviewer note

Status: proposed
"
                ),
            )
            .map_err(|error| error.to_string())?;
            let mut history = HistoryInput {
                repository_available: true,
                source: "fixture".to_string(),
                ..HistoryInput::default()
            };
            history
                .last_changed
                .insert(relative.to_string(), last_changed.to_string());
            let report = build_report(&root, "2026-08-27", &history)?;
            let row = report
                .specs
                .first()
                .ok_or_else(|| "missing fixture row".to_string())?;
            Ok((row
                .reason_codes
                .contains(&"periodic_attention_due".to_string()),))
        };

        let at_threshold = build("2025-08-27")?.0;
        let below_threshold = build("2025-08-28")?.0;
        if !at_threshold {
            return Err("365 days did not trigger periodic_attention_due".to_string());
        }
        if below_threshold {
            return Err("364 days unexpectedly triggered periodic_attention_due".to_string());
        }
        Ok(())
    }

    #[test]
    fn zero_and_many_denominators_succeed_and_render_from_one_dto() -> Result<(), String> {
        let empty = fixture_root()?;
        let empty_report = build_report(&empty, "2026-08-27", &HistoryInput::default())?;
        if empty_report.denominator.included != 0 || empty_report.denominator.omitted.len() != 1 {
            return Err("zero-candidate denominator was not reported".to_string());
        }
        let root = fixture_root()?;
        for id in ["0001", "0002"] {
            fs::write(
                root.join(format!("docs/specs/RIPR-SPEC-{id}-example.md")),
                "# Example\n\nStatus: planned\n",
            )
            .map_err(|error| error.to_string())?;
        }
        let report = build_report(&root, "2026-08-27", &HistoryInput::default())?;
        let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
        let decoded: SpecMaintenanceReportV1 =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        if decoded != report || render_markdown(&decoded).is_empty() || report.specs.len() != 2 {
            return Err("JSON/Markdown did not use the same DTO or many case failed".to_string());
        }
        Ok(())
    }

    #[test]
    fn malformed_as_of_is_rejected() -> Result<(), String> {
        if validate_date("2026-02-31").is_ok() {
            return Err("invalid calendar date unexpectedly accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn malformed_required_source_fails_visibly() -> Result<(), String> {
        let root = fixture_root()?;
        fs::write(
            root.join("docs/specs/RIPR-SPEC-9999-example.md"),
            [0xff, 0xfe],
        )
        .map_err(|error| error.to_string())?;
        if build_report(&root, "2026-08-27", &HistoryInput::default()).is_ok() {
            return Err("invalid UTF-8 unexpectedly succeeded".to_string());
        }
        Ok(())
    }
}

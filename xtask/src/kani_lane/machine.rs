//! Machine-readable observations from the existing Kani runner (#14472).
//!
//! This is an observation artifact, not a registered-claim receipt. Native
//! verdicts remain owned by the shared interpretation module. Captured UTF-8 text is retained
//! explicitly as such; the process helper does not expose original pipe bytes.

use super::{Harness, HarnessResult, KaniArgs};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
const MAX_RUN_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CAPTURED_SOURCE_BYTES: usize = 64 * 1024 * 1024;
static NEXT_RUN: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SourceObservation {
    file: String,
    bytes: usize,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CapturedText {
    encoding: String,
    bytes: usize,
    sha256: String,
    text: Option<String>,
}

impl CapturedText {
    fn new(text: &str) -> Self {
        Self {
            encoding: "utf8_text_from_process_capture".to_string(),
            bytes: text.len(),
            sha256: digest(text.as_bytes()),
            text: (text.len() <= MAX_STREAM_BYTES).then(|| text.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct NativeBlock {
    qualified_name: String,
    normalized_block_sha256: String,
    covers: Option<super::CoverSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Invocation {
    package: String,
    argv: Vec<String>,
    timeout_ms: Option<u64>,
    elapsed_ms: Option<u64>,
    exit_code: Option<i32>,
    exit_success: Option<bool>,
    timed_out: bool,
    capture_error: Option<String>,
    cleanup_state: String,
    stdout: Option<CapturedText>,
    stderr: Option<CapturedText>,
    native_blocks: Vec<NativeBlock>,
    unmatched_native_blocks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Row {
    package: String,
    observation: HarnessResult,
    claim_id: Option<String>,
    claim_generation: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Selected {
    package: String,
    harness: Harness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Report {
    schema: String,
    run_id: String,
    phase: String,
    requested: Option<KaniArgs>,
    tool_available: Option<bool>,
    lane_success: bool,
    lane_error: Option<String>,
    selected: Vec<Selected>,
    selected_source_files: Vec<SourceObservation>,
    selected_files_unchanged_after_run: Option<bool>,
    source_observation_digest: Option<String>,
    source_recheck_errors: Vec<String>,
    invocations: Vec<Invocation>,
    results: Vec<Row>,
    registered_claims: usize,
    claim_assurance: String,
    limitations: Vec<String>,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn millis(duration: Duration) -> Option<u64> {
    u64::try_from(duration.as_millis()).ok()
}

impl Report {
    pub(super) fn new(run_id: String) -> Self {
        Self {
            schema: "ripr.kani-observation.v1".to_string(),
            run_id,
            phase: "started".to_string(),
            requested: None,
            tool_available: None,
            lane_success: false,
            lane_error: Some("run has not completed".to_string()),
            selected: Vec::new(),
            selected_source_files: Vec::new(),
            selected_files_unchanged_after_run: None,
            source_observation_digest: None,
            source_recheck_errors: Vec::new(),
            invocations: Vec::new(),
            results: Vec::new(),
            registered_claims: 0,
            claim_assurance: "not_established".to_string(),
            limitations: [
                "Harnesses are unregistered; no stable claim ID or claim generation is invented.",
                "Discovery is the existing lexical source scan, not complete native selection.",
                "Unique-name result joins do not establish exact native module/source identity.",
                "Source digests cover discovered proof files only, not complete consumed dependencies/configuration.",
                "Before/after file equality is not atomic source custody or proof of bytes consumed by the verifier.",
                "Captured text may have been lossily decoded by the existing process helper; hashes do not commit original pipe bytes.",
                "Provider/solver/toolchain identity and process-tree cleanup are not independently established.",
                "Preflight bounding remains owned by #12581; this artifact does not introduce a second probe executor.",
                "A native verdict does not establish specification adequacy, registered assurance or merge authority.",
            ].map(str::to_string).to_vec(),
        }
    }

    pub(super) fn run_id(&self) -> &str {
        &self.run_id
    }

    pub(super) fn request(&mut self, args: &KaniArgs) {
        self.requested = Some(args.clone());
    }

    pub(super) fn select(
        &mut self,
        package: &str,
        files: &[(String, String)],
        harnesses: &[Harness],
    ) -> Result<(), String> {
        // Hash exactly the valid UTF-8 source bytes the existing discovery saw.
        let mut sources: BTreeMap<String, SourceObservation> = self
            .selected_source_files
            .iter()
            .map(|s| (s.file.clone(), s.clone()))
            .collect();
        for (file, source) in files {
            if !harnesses.iter().any(|h| h.file == *file) {
                continue;
            }
            if source.len() > MAX_SOURCE_BYTES as usize {
                return Err("Kani source observation exceeds its per-file byte budget".to_string());
            }
            let observation = SourceObservation {
                file: file.clone(),
                bytes: source.len(),
                sha256: digest(source.as_bytes()),
            };
            if sources.get(file).is_some_and(|old| old != &observation) {
                return Err("conflicting source bytes for one Kani inventory path".to_string());
            }
            sources.insert(file.clone(), observation);
        }
        let bytes = sources
            .values()
            .try_fold(0usize, |total, item| total.checked_add(item.bytes))
            .ok_or("source accounting overflow")?;
        if bytes > MAX_CAPTURED_SOURCE_BYTES {
            return Err(
                "Kani source observation exceeds its total inventory byte budget".to_string(),
            );
        }
        self.selected_source_files = sources.into_values().collect();
        // Sorted typed records and JSON escaping avoid delimiter ambiguity.
        let encoded = serde_json::to_vec(&self.selected_source_files).map_err(|e| e.to_string())?;
        self.source_observation_digest = Some(digest(&encoded));
        self.selected
            .extend(harnesses.iter().cloned().map(|harness| Selected {
                package: package.to_string(),
                harness,
            }));
        self.selected.sort_by(|a, b| {
            (&a.package, &a.harness.file, &a.harness.name).cmp(&(
                &b.package,
                &b.harness.file,
                &b.harness.name,
            ))
        });
        Ok(())
    }

    pub(super) fn availability(&mut self, available: bool) {
        self.tool_available = Some(available);
    }

    /// Capture the exact strings returned by the existing subprocess owner.
    pub(super) fn capture(&mut self, context: InvocationContext<'_>, outcome: CaptureOutcome<'_>) {
        let InvocationContext {
            package,
            argv,
            budget,
            elapsed,
            harnesses,
        } = context;
        let CaptureOutcome {
            exit_code,
            exit_success,
            timed_out,
            stdout,
            stderr,
            error,
        } = outcome;
        let combined = format!("{}\n{}", stdout.unwrap_or(""), stderr.unwrap_or(""));
        let blocks = if package == "@preflight" {
            Vec::new()
        } else {
            super::split_harness_blocks(&combined)
        };
        let mut unmatched: Vec<String> = blocks
            .iter()
            .filter(|(name, _)| !harnesses.iter().any(|h| super::matches_harness(name, h)))
            .map(|(name, _)| name.clone())
            .collect();
        unmatched.sort();
        let mut native_blocks: Vec<NativeBlock> = blocks
            .into_iter()
            .map(|(name, text)| {
                // This legacy document commits normalized lines. The shared
                // parser retains original block bytes for the native DTO.
                let normalized: String = text.lines().map(|line| format!("{line}\n")).collect();
                let summaries: Vec<&str> = text
                    .lines()
                    .filter(|line| line.contains("cover properties satisfied"))
                    .collect();
                let covers = if summaries.len() == 1 {
                    super::parse_cover_summary(summaries[0])
                } else {
                    None
                };
                NativeBlock {
                    qualified_name: name,
                    normalized_block_sha256: digest(normalized.as_bytes()),
                    covers,
                }
            })
            .collect();
        native_blocks.sort_by(|a, b| {
            (&a.qualified_name, &a.normalized_block_sha256)
                .cmp(&(&b.qualified_name, &b.normalized_block_sha256))
        });
        let retained = self
            .invocations
            .iter()
            .flat_map(|i| [i.stdout.as_ref(), i.stderr.as_ref()])
            .flatten()
            .filter_map(|s| s.text.as_ref())
            .map(String::len)
            .sum::<usize>();
        let mut stdout_capture = stdout.map(CapturedText::new);
        let mut stderr_capture = stderr.map(CapturedText::new);
        let added = stdout_capture
            .as_ref()
            .and_then(|s| s.text.as_ref())
            .map_or(0, String::len)
            + stderr_capture
                .as_ref()
                .and_then(|s| s.text.as_ref())
                .map_or(0, String::len);
        if retained.saturating_add(added) > MAX_RUN_TEXT_BYTES {
            if let Some(capture) = &mut stdout_capture {
                capture.text = None;
            }
            if let Some(capture) = &mut stderr_capture {
                capture.text = None;
            }
        }
        self.invocations.push(Invocation {
            package: package.to_string(),
            argv: argv.to_vec(),
            timeout_ms: budget.and_then(millis),
            elapsed_ms: millis(elapsed),
            exit_code,
            exit_success,
            timed_out,
            capture_error: error.map(str::to_string),
            cleanup_state: "not_observed".to_string(),
            stdout: stdout_capture,
            stderr: stderr_capture,
            native_blocks,
            unmatched_native_blocks: unmatched,
        });
    }

    pub(super) fn rows(&mut self, package: &str, rows: &[HarnessResult]) {
        self.results
            .extend(rows.iter().cloned().map(|observation| Row {
                package: package.to_string(),
                observation,
                claim_id: None,
                claim_generation: None,
            }));
        self.results.sort_by(|a, b| {
            (
                &a.package,
                &a.observation.harness.file,
                &a.observation.harness.name,
            )
                .cmp(&(
                    &b.package,
                    &b.observation.harness.file,
                    &b.observation.harness.name,
                ))
        });
    }

    pub(super) fn recheck_sources(&mut self) {
        self.source_recheck_errors.clear();
        for source in &self.selected_source_files {
            match read_bounded(Path::new(&source.file)) {
                Ok(bytes) if digest(&bytes) == source.sha256 => {}
                Ok(_) => self
                    .source_recheck_errors
                    .push(format!("source changed: {}", source.file)),
                Err(reason) => self
                    .source_recheck_errors
                    .push(format!("source recheck failed: {}: {reason}", source.file)),
            }
        }
        self.selected_files_unchanged_after_run = (!self.selected_source_files.is_empty())
            .then_some(self.source_recheck_errors.is_empty());
    }

    fn evidence_problem(&self) -> Option<String> {
        if self.tool_available != Some(true) {
            return Some("successful tool availability was not observed".to_string());
        }
        if self.requested.is_none() {
            return Some("no valid Kani request was recorded".to_string());
        }
        if self.selected.is_empty() && !self.requested.as_ref().is_some_and(|r| r.allow_empty) {
            return Some("no source harnesses selected".to_string());
        }
        // File custody must cover exactly the selected source-file set. A
        // successful recheck of one observed file says nothing about a second
        // selected file omitted from discovery's supplied source observations.
        // Several distinct harnesses may legitimately share one source file.
        let selected_files: BTreeSet<&str> = self
            .selected
            .iter()
            .map(|selected| selected.harness.file.as_str())
            .collect();
        let observed_files: BTreeSet<&str> = self
            .selected_source_files
            .iter()
            .map(|source| source.file.as_str())
            .collect();
        if selected_files != observed_files {
            return Some(
                "selected harness files and observed source files do not agree".to_string(),
            );
        }
        if observed_files.len() != self.selected_source_files.len() {
            return Some("duplicate source-file observations are not admitted".to_string());
        }
        if !self.source_recheck_errors.is_empty()
            || (!self.selected.is_empty() && self.selected_files_unchanged_after_run != Some(true))
        {
            return Some("discovered proof source changed or could not be rechecked".to_string());
        }
        let selected: Vec<_> = self
            .selected
            .iter()
            .map(|s| (&s.package, &s.harness.file, &s.harness.name))
            .collect();
        let returned: Vec<_> = self
            .results
            .iter()
            .map(|r| {
                (
                    &r.package,
                    &r.observation.harness.file,
                    &r.observation.harness.name,
                )
            })
            .collect();
        if selected != returned || selected.iter().collect::<BTreeSet<_>>().len() != selected.len()
        {
            return Some(
                "selected and reported source-harness inventories do not agree uniquely"
                    .to_string(),
            );
        }
        let mut invocation_packages = BTreeSet::new();
        for invocation in &self.invocations {
            if !invocation_packages.insert(invocation.package.as_str()) {
                return Some(
                    "multiple invocations for one package are not an admitted observation"
                        .to_string(),
                );
            }
            if invocation.exit_success != Some(true)
                || invocation.timed_out
                || invocation.capture_error.is_some()
            {
                return Some("a subprocess did not complete successfully".to_string());
            }
            if !invocation.unmatched_native_blocks.is_empty() {
                return Some(
                    "native output contains harness blocks outside the source inventory"
                        .to_string(),
                );
            }
            if invocation.stdout.as_ref().is_some_and(|s| s.text.is_none())
                || invocation.stderr.as_ref().is_some_and(|s| s.text.is_none())
            {
                return Some("captured text exceeded the per-stream retention budget".to_string());
            }
        }
        if !invocation_packages.contains("@preflight")
            || self
                .selected
                .iter()
                .any(|s| !invocation_packages.contains(s.package.as_str()))
        {
            return Some("a selected invocation has no process observation".to_string());
        }
        None
    }

    /// Only the existing lane decision may admit execution. Reporting can
    /// refuse incomplete evidence, but cannot turn a failed lane into success.
    pub(super) fn finish(&mut self, lane: Result<(), String>) -> Result<(), String> {
        self.phase = "completed".to_string();
        let result = match (lane, self.evidence_problem()) {
            (Ok(()), None) => Ok(()),
            (Err(error), None) => Err(error),
            (Ok(()), Some(error)) => Err(format!("NOT-ESTABLISHED: {error}")),
            (Err(error), Some(problem)) => Err(format!("{error}; NOT-ESTABLISHED: {problem}")),
        };
        self.lane_success = result.is_ok();
        self.lane_error = result.as_ref().err().cloned();
        result
    }

    fn markdown(&self) -> String {
        let rows: Vec<_> = self.results.iter().map(|r| r.observation.clone()).collect();
        let note = format!(
            "Run ID: `{}`. Phase: `{}`. Lane success: `{}`. Registered claims: 0.\n\n{}",
            self.run_id,
            self.phase,
            self.lane_success,
            escape(
                self.lane_error
                    .as_deref()
                    .unwrap_or("No lane failure observed.")
            )
        );
        let mut text = super::render_report(&rows, &note);
        text.push_str("\n## Package-qualified source inventory\n\n| Package | Source file | Harness |\n| --- | --- | --- |\n");
        for item in &self.selected {
            text.push_str(&format!(
                "| {} | {} | {} |\n",
                escape(&item.package),
                escape(&item.harness.file),
                escape(&item.harness.name)
            ));
        }
        text.push_str("\nNative observations are not registered assurance. Source-file equality does not establish complete source custody. See kani.json for process observations, captured-text commitments and limitations.\n");
        text
    }
}

pub(super) struct InvocationContext<'a> {
    pub(super) package: &'a str,
    pub(super) argv: &'a [String],
    pub(super) budget: Option<Duration>,
    pub(super) elapsed: Duration,
    pub(super) harnesses: &'a [Harness],
}

pub(super) struct CaptureOutcome<'a> {
    pub(super) exit_code: Option<i32>,
    pub(super) exit_success: Option<bool>,
    pub(super) timed_out: bool,
    pub(super) stdout: Option<&'a str>,
    pub(super) stderr: Option<&'a str>,
    pub(super) error: Option<&'a str>,
}

pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace('`', "&#96;")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("source recheck requires a regular, non-symlink file".to_string());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err("source exceeds recheck byte budget".to_string());
    }
    Ok(bytes)
}

/// An immutable pair per checkpoint, plus replaceable convenience aliases.
/// Readers must bind an expected run ID; aliases are last-published, not an
/// assertion of which concurrently started invocation is newest/current.
pub(super) struct Journal {
    root: PathBuf,
    directory: PathBuf,
    sequence: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Published {
    report: Report,
    markdown_path: String,
    markdown: String,
    markdown_sha256: String,
}

impl Journal {
    pub(super) fn start(root: &Path) -> Result<(Self, Report), String> {
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let sequence = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
        let run_id = format!("kani-{}-{tick}-{sequence}", std::process::id());
        fs::create_dir_all(root.join("kani-runs")).map_err(|e| e.to_string())?;
        let directory = root.join("kani-runs").join(&run_id);
        // Never reuse an existing run directory or arm cleanup for an unowned one.
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let report = Report::new(run_id);
        let mut journal = Self {
            root: root.to_path_buf(),
            directory,
            sequence: 0,
        };
        journal.publish(&report)?;
        Ok((journal, report))
    }

    pub(super) fn publish(&mut self, report: &Report) -> Result<String, String> {
        let sequence = self.sequence;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("checkpoint counter exhausted")?;
        let markdown = report.markdown();
        let markdown_path = format!("kani-runs/{}/checkpoint-{sequence}.md", report.run_id);
        let document = Published {
            report: report.clone(),
            markdown_path,
            markdown: markdown.clone(),
            markdown_sha256: digest(markdown.as_bytes()),
        };
        let json = serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?;
        // Decode our own authoritative DTO before publishing any terminal alias.
        let decoded: Published = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
        if decoded.report != *report
            || decoded.markdown != markdown
            || decoded.markdown_sha256 != digest(markdown.as_bytes())
        {
            return Err("Kani report serialization changed the observed evidence".to_string());
        }
        write_new(
            &self.directory.join(format!("checkpoint-{sequence}.md")),
            markdown.as_bytes(),
        )?;
        write_new(
            &self.directory.join(format!("checkpoint-{sequence}.json")),
            &json,
        )?;
        // Publish Markdown first; JSON is the commit point and names its own
        // immutable Markdown. A reader never has to trust a mismatched alias.
        self.replace_alias(sequence, "kani.md", markdown.as_bytes())?;
        self.replace_alias(sequence, "kani.json", &json)?;
        Ok(markdown)
    }

    fn replace_alias(&self, sequence: u64, name: &str, bytes: &[u8]) -> Result<(), String> {
        let staging = self.directory.join(format!("alias-{sequence}-{name}.tmp"));
        write_new(&staging, bytes)?;
        fs::rename(&staging, self.root.join(name)).map_err(|e| format!("publish {name}: {e}"))
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::super::HarnessStatus;
    use super::*;

    fn ready(report: &mut Report, allow_empty: bool) -> Result<(), String> {
        let arguments = if allow_empty {
            vec!["--allow-empty".to_string()]
        } else {
            Vec::new()
        };
        report.request(&super::super::parse_args(&arguments)?);
        report.availability(true);
        report.capture(
            InvocationContext {
                package: "@preflight",
                argv: &[],
                budget: None,
                elapsed: Duration::ZERO,
                harnesses: &[],
            },
            CaptureOutcome {
                exit_code: Some(0),
                exit_success: Some(true),
                timed_out: false,
                stdout: Some("synthetic test probe"),
                stderr: Some(""),
                error: None,
            },
        );
        Ok(())
    }

    fn capture_success(report: &mut Report, output: &str, harnesses: &[Harness]) {
        report.capture(
            InvocationContext {
                package: "ripr",
                argv: &[],
                budget: Some(Duration::from_secs(1)),
                elapsed: Duration::ZERO,
                harnesses,
            },
            CaptureOutcome {
                exit_code: Some(0),
                exit_success: Some(true),
                timed_out: false,
                stdout: Some(output),
                stderr: Some(""),
                error: None,
            },
        );
    }

    fn harness(name: &str) -> Harness {
        Harness {
            file: "proof.rs".to_string(),
            name: name.to_string(),
        }
    }

    fn selected(name: &str) -> Result<Report, String> {
        let mut report = Report::new("synthetic-only".to_string());
        report.select(
            "ripr",
            &[("proof.rs".to_string(), "test source".to_string())],
            &[harness(name)],
        )?;
        ready(&mut report, false)?;
        // This helper models a completed file recheck; the process tests exercise
        // actual reads. The missing-recheck control removes this observation.
        report.selected_files_unchanged_after_run = Some(true);
        Ok(report)
    }

    fn good_output(name: &str) -> String {
        format!(
            "Checking harness m::{name}...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n"
        )
    }

    #[test]
    fn missing_tool_keeps_expected_subject_and_refuses_success() -> Result<(), String> {
        let mut report = selected("proof")?;
        report.availability(false);
        report.rows(
            "ripr",
            &super::super::all_not_established(&[harness("proof")], "tool unavailable"),
        );
        assert!(report.finish(Err("tool unavailable".to_string())).is_err());
        assert_eq!(report.selected.len(), 1);
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].claim_id, None);
        assert_eq!(report.invocations.len(), 1);
        Ok(())
    }

    #[test]
    fn partial_timeout_preserves_native_row_without_lane_success() -> Result<(), String> {
        let mut report = selected("proof")?;
        let output = good_output("proof");
        report.capture(
            InvocationContext {
                package: "ripr",
                argv: &["kani".to_string()],
                budget: Some(Duration::from_secs(1)),
                elapsed: Duration::from_secs(1),
                harnesses: &[harness("proof")],
            },
            CaptureOutcome {
                exit_code: None,
                exit_success: None,
                timed_out: true,
                stdout: Some(&output),
                stderr: Some(""),
                error: None,
            },
        );
        report.rows(
            "ripr",
            &super::super::results_from_output(&[harness("proof")], &output),
        );
        assert!(report.finish(Err("timed out".to_string())).is_err());
        assert!(matches!(
            report.results[0].observation.status,
            HarnessStatus::Verified
        ));
        assert_eq!(report.claim_assurance, "not_established");
        assert_eq!(report.invocations[1].exit_code, None);
        Ok(())
    }

    #[test]
    fn native_extra_subject_is_not_silently_dropped() -> Result<(), String> {
        let mut report = selected("proof")?;
        let output = format!("{}{}", good_output("proof"), good_output("unexpected"));
        report.capture(
            InvocationContext {
                package: "ripr",
                argv: &[],
                budget: None,
                elapsed: Duration::ZERO,
                harnesses: &[harness("proof")],
            },
            CaptureOutcome {
                exit_code: Some(0),
                exit_success: Some(true),
                timed_out: false,
                stdout: Some(&output),
                stderr: Some(""),
                error: None,
            },
        );
        report.rows(
            "ripr",
            &super::super::results_from_output(&[harness("proof")], &output),
        );
        assert!(report.finish(Ok(())).is_err());
        assert_eq!(
            report.invocations[1].unmatched_native_blocks,
            ["m::unexpected"]
        );
        Ok(())
    }

    #[test]
    fn duplicate_selected_or_missing_rows_cannot_pass() -> Result<(), String> {
        let mut report = selected("proof")?;
        assert!(report.finish(Ok(())).is_err());
        report.selected.push(report.selected[0].clone());
        let output = good_output("proof");
        report.rows(
            "ripr",
            &super::super::results_from_output(&[harness("proof"), harness("proof")], &output),
        );
        assert!(report.finish(Ok(())).is_err());
        Ok(())
    }

    #[test]
    fn an_omitted_source_recheck_cannot_admit_a_clean_run() -> Result<(), String> {
        let mut report = selected("proof")?;
        capture_success(&mut report, &good_output("proof"), &[harness("proof")]);
        report.rows(
            "ripr",
            &super::super::results_from_output(&[harness("proof")], &good_output("proof")),
        );
        report.selected_files_unchanged_after_run = None;
        assert!(report.finish(Ok(())).is_err());
        Ok(())
    }

    #[test]
    fn retention_limit_keeps_hash_and_refuses_complete_evidence() {
        let text = "x".repeat(MAX_STREAM_BYTES + 1);
        let capture = CapturedText::new(&text);
        assert_eq!(capture.text, None);
        assert_eq!(capture.bytes, text.len());
        assert_eq!(capture.sha256, digest(text.as_bytes()));
    }

    #[test]
    fn markdown_cells_do_not_execute_control_or_markup_text() {
        let escaped = escape("<tag>|`value`\n\u{1b}[2J");
        assert!(!escaped.chars().any(char::is_control));
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('|'));
        assert!(!escaped.contains('`'));
    }

    #[test]
    fn source_inventory_digest_is_order_independent() -> Result<(), String> {
        let files = [
            ("b.rs".to_string(), "b".to_string()),
            ("a.rs".to_string(), "a".to_string()),
        ];
        let mut a = Report::new("test".to_string());
        let mut b = Report::new("test".to_string());
        let hs = [
            Harness {
                file: "a.rs".to_string(),
                name: "a".to_string(),
            },
            Harness {
                file: "b.rs".to_string(),
                name: "b".to_string(),
            },
        ];
        a.select("ripr", &files, &hs)?;
        b.select(
            "ripr",
            &[files[1].clone(), files[0].clone()],
            &[hs[1].clone(), hs[0].clone()],
        )?;
        assert_eq!(a, b);
        Ok(())
    }

    #[test]
    fn observed_cover_counts_use_existing_parser() -> Result<(), String> {
        let mut report = selected("proof")?;
        let output = "Checking harness m::proof...\n ** 1 of 3 cover properties satisfied (1 undetermined,1 unreachable)\nVERIFICATION:- FAILED\n";
        report.capture(
            InvocationContext {
                package: "ripr",
                argv: &[],
                budget: None,
                elapsed: Duration::ZERO,
                harnesses: &[harness("proof")],
            },
            CaptureOutcome {
                exit_code: Some(1),
                exit_success: Some(false),
                timed_out: false,
                stdout: Some(output),
                stderr: Some(""),
                error: None,
            },
        );
        let covers = report.invocations[1].native_blocks[0]
            .covers
            .as_ref()
            .ok_or("missing counts")?;
        assert_eq!(
            (
                covers.satisfied,
                covers.total,
                covers.undetermined,
                covers.unreachable
            ),
            (1, 3, 1, 1)
        );
        Ok(())
    }

    #[test]
    fn journal_replaces_old_success_with_new_incomplete_run() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "kani-machine-test-{}-{}",
            std::process::id(),
            NEXT_RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let result = (|| {
            let (mut first, mut report) = Journal::start(&root)?;
            ready(&mut report, true)?;
            report.finish(Ok(()))?;
            first.publish(&report)?;
            let (_second, pending) = Journal::start(&root)?;
            let latest: Published = serde_json::from_slice(
                &fs::read(root.join("kani.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            assert_ne!(latest.report.run_id, report.run_id);
            assert_eq!(latest.report.run_id, pending.run_id);
            assert!(!latest.report.lane_success);
            let md = fs::read(root.join(&latest.markdown_path)).map_err(|e| e.to_string())?;
            assert_eq!(digest(&md), latest.markdown_sha256);
            assert!(first.directory.join("checkpoint-1.json").exists());
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
        result
    }

    #[test]
    fn source_change_stops_lane_without_erasing_historical_rows() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "kani-source-test-{}-{}",
            std::process::id(),
            NEXT_RUN.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let result = (|| {
            let path = root.join("proof.rs");
            fs::write(&path, b"before").map_err(|e| e.to_string())?;
            let file = path.to_string_lossy().into_owned();
            let h = Harness {
                file: file.clone(),
                name: "proof".to_string(),
            };
            let mut report = Report::new("test".to_string());
            ready(&mut report, false)?;
            report.select(
                "ripr",
                &[(file, "before".to_string())],
                std::slice::from_ref(&h),
            )?;
            capture_success(&mut report, &good_output("proof"), std::slice::from_ref(&h));
            report.rows(
                "ripr",
                &super::super::results_from_output(&[h], &good_output("proof")),
            );
            fs::write(&path, b"after").map_err(|e| e.to_string())?;
            report.recheck_sources();
            assert_eq!(report.selected_files_unchanged_after_run, Some(false));
            assert!(report.finish(Ok(())).is_err());
            assert!(matches!(
                report.results[0].observation.status,
                HarnessStatus::Verified
            ));
            Ok(())
        })();
        fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
        result
    }

    #[test]
    fn markdown_is_derived_from_serialized_report_rows() -> Result<(), String> {
        let mut report = selected("proof")?;
        capture_success(&mut report, &good_output("proof"), &[harness("proof")]);
        report.rows(
            "ripr",
            &super::super::results_from_output(&[harness("proof")], &good_output("proof")),
        );
        report.finish(Ok(()))?;
        let serialized = serde_json::to_vec(&report).map_err(|e| e.to_string())?;
        let decoded: Report = serde_json::from_slice(&serialized).map_err(|e| e.to_string())?;
        assert_eq!(decoded.markdown(), report.markdown());
        assert!(
            decoded
                .markdown()
                .contains("| `proof` | `proof.rs` | VERIFIED |")
        );
        assert_eq!(decoded.registered_claims, 0);
        Ok(())
    }
}

#[cfg(test)]
mod inventory_tests;

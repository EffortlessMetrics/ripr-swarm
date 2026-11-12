//! Synthetic-diff verdict join for languages ripr has no repo-exposure seams
//! for (TypeScript, and later Python).
//!
//! `ripr check --format repo-exposure-json` lists no seams for these
//! languages (their analysis is diff-first), so the Rust seam-span join of
//! `ripr calibrate cargo-mutants` cannot score them. This module scores ripr's
//! diff-time verdict against a real mutation tool's outcomes instead:
//!
//! 1. each mutant of a mutation-tool report is expressed as a
//!    [`SyntheticMutant`] (file, replaced range, replacement text, outcome);
//! 2. a one-hunk unified diff that turns the original source into the mutated
//!    source is synthesized ([`synthesize_edit`]);
//! 3. the mutated file is written into a scratch copy of the checkout, whose
//!    `ripr.toml` explicitly enables the language, and
//!    `ripr check --diff <hunk> --format json` runs there;
//! 4. the finding on the mutated line range is read back ([`read_verdict`]).
//!
//! Only unambiguous pairs are scored: a runtime outcome of `caught` or
//! `missed` and exactly one verdict (one class, one owner) on the mutated
//! lines. A strongly exposed verdict claims a discriminator; a
//! `reachable_unrevealed` or `no_static_path` verdict claims none;
//! `weakly_exposed` and the unknown classes make no claim one mutant settles.
//! Everything else is counted under an explicit exclusion reason, so
//! `canonical_precise` plus the exclusions equals the mutant count.
//!
//! The receipt labels these repos `agreement_method: "synthetic_diff_verdict"`
//! and keeps them out of the Rust span-join aggregates.
//!
//! The join is language-generic: a parser for another mutation tool (for
//! example cosmic-ray or mutmut for Python) only has to produce
//! [`MutationSource`] values and pick a [`LanguageProfile`].

use super::{CANONICAL_PRECISE, Record, RepoRun, SCOREABLE_OUTCOMES, path_arg, run_text};
use crate::run::capture_output_with_timeout;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Receipt label for the per-repo `agreement_method`.
pub(crate) const AGREEMENT_METHOD: &str = "synthetic_diff_verdict";
/// Mutants beyond this many that reach the ripr step are excluded as
/// `over_mutant_cap`, so a very large report cannot run for hours.
pub(crate) const DEFAULT_MAX_MUTANTS: usize = 1500;
const CHECK_TIMEOUT: Duration = Duration::from_mins(2);
/// Directories never copied into the scratch tree.
const COPY_SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".stryker-tmp",
    "coverage",
    "__pycache__",
    ".venv",
    ".tox",
    ".pytest_cache",
];

const NON_SOURCE_FILE: &str = "non_target_language_file";
const SOURCE_MISMATCH: &str = "source_mismatch";
const UNSCOREABLE_OUTCOME: &str = "unscoreable_outcome";
const INVALID_LOCATION: &str = "invalid_location";
const NOOP_MUTANT: &str = "noop_mutant";
const OVER_CAP: &str = "over_mutant_cap";
const CHECK_FAILED: &str = "ripr_check_failed";
const ANALYSIS_INCOMPLETE: &str = "analysis_incomplete";
const NO_FINDING: &str = "no_finding_on_mutated_lines";
const AMBIGUOUS_VERDICT: &str = "ambiguous_verdict";
const SYNTHETIC_SEAM_KIND: &str = "diff_probe";
pub(crate) const RECORDED_DIFF_MISSING: &str = "recorded_diff_missing";

/// The language a mutation report is scored for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LanguageProfile {
    /// Wire name used in `ripr.toml` `[languages] enabled` and in the receipt.
    pub(crate) language: &'static str,
    /// Lowercase file extensions (no dot) the language's adapter analyzes.
    pub(crate) extensions: &'static [&'static str],
    /// Build-output directories skipped at the checkout root when copying the
    /// scratch tree, on top of the shared skip list. Only the root: a source
    /// module directory that happens to share a name (`src/target/`) is kept.
    pub(crate) skip_dirs: &'static [&'static str],
    /// A mutant without a recorded diff is excluded as
    /// `recorded_diff_missing` instead of being rebuilt from its span, because
    /// the tool's edit cannot be reconstructed faithfully (cargo-mutants
    /// before 27 records no diff).
    pub(crate) requires_recorded_diff: bool,
}

pub(crate) const TYPESCRIPT: LanguageProfile = LanguageProfile {
    language: "typescript",
    extensions: &["ts", "tsx", "mts", "cts"],
    skip_dirs: &[],
    requires_recorded_diff: false,
};

pub(crate) const PYTHON: LanguageProfile = LanguageProfile {
    language: "python",
    extensions: &["py"],
    skip_dirs: &[],
    requires_recorded_diff: false,
};

/// Rust scored through the diff path with each cargo-mutants mutant's own
/// recorded diff (see [`super::cargo_mutants_diff`]).
pub(crate) const RUST: LanguageProfile = LanguageProfile {
    language: "rust",
    extensions: &["rs"],
    skip_dirs: &["target", "mutants.out", "mutants.out.old"],
    requires_recorded_diff: true,
};

/// One mutant in the join's language-neutral form.
///
/// `start_col` and `end_col` are 0-based UTF-16 code-unit offsets within
/// their (1-based) lines, `end_col` exclusive, which is how JavaScript tools
/// count; a parser converts its tool's convention into this one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntheticMutant {
    pub(crate) id: String,
    pub(crate) mutator: String,
    pub(crate) replacement: String,
    pub(crate) start_line: u64,
    pub(crate) start_col: u64,
    pub(crate) end_line: u64,
    pub(crate) end_col: u64,
    /// The tool's status, for diagnostics.
    pub(crate) status: String,
    /// `caught`, `missed`, or another token that is never scored.
    pub(crate) outcome: String,
    /// The tool's own unified diff for this mutant (cargo-mutants records
    /// one); when present it is applied instead of synthesizing an edit.
    pub(crate) recorded_diff: Option<String>,
}

/// A source file the report mutated, with the text the tool mutated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MutationSource {
    /// Path relative to the checkout, `/`-separated.
    pub(crate) path: String,
    pub(crate) source: String,
    pub(crate) mutants: Vec<SyntheticMutant>,
}

/// A mutant turned into a one-hunk diff plus the full mutated file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntheticEdit {
    pub(crate) diff: String,
    pub(crate) mutated: String,
    /// Mutated lines in the new file, inclusive and 1-based.
    pub(crate) first_line: u64,
    pub(crate) last_line: u64,
}

/// Byte offset of UTF-16 column `col` in `line`, or `None` when it falls past
/// the line or inside a surrogate pair.
fn utf16_to_byte(line: &str, col: u64) -> Option<usize> {
    let mut units = 0u64;
    for (index, ch) in line.char_indices() {
        if units == col {
            return Some(index);
        }
        if units > col {
            return None;
        }
        units += ch.len_utf16() as u64;
    }
    (units == col).then_some(line.len())
}

/// Apply `mutant` to `source` and describe the change as a one-hunk diff.
/// The error is the exclusion reason.
pub(crate) fn synthesize_edit(
    path: &str,
    source: &str,
    mutant: &SyntheticMutant,
) -> Result<SyntheticEdit, &'static str> {
    let ends_with_newline = source.ends_with('\n');
    let mut lines: Vec<&str> = source.split('\n').collect();
    if ends_with_newline {
        lines.pop();
    }
    let first = usize::try_from(mutant.start_line)
        .ok()
        .and_then(|line| line.checked_sub(1))
        .ok_or(INVALID_LOCATION)?;
    let last = usize::try_from(mutant.end_line)
        .ok()
        .and_then(|line| line.checked_sub(1))
        .ok_or(INVALID_LOCATION)?;
    if first > last || last >= lines.len() {
        return Err(INVALID_LOCATION);
    }
    let head = utf16_to_byte(lines[first], mutant.start_col).ok_or(INVALID_LOCATION)?;
    let tail = utf16_to_byte(lines[last], mutant.end_col).ok_or(INVALID_LOCATION)?;
    if first == last && tail < head {
        return Err(INVALID_LOCATION);
    }
    let new_text = format!(
        "{}{}{}",
        &lines[first][..head],
        mutant.replacement,
        &lines[last][tail..]
    );
    let old_lines = &lines[first..=last];
    let new_lines: Vec<&str> = new_text.split('\n').collect();
    if old_lines == new_lines.as_slice() {
        return Err(NOOP_MUTANT);
    }
    let no_newline_at_eof = !ends_with_newline && last + 1 == lines.len();
    let marker = "\\ No newline at end of file\n";
    let mut diff = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{start},{old} +{start},{new} @@\n",
        start = first + 1,
        old = old_lines.len(),
        new = new_lines.len(),
    );
    for line in old_lines {
        diff.push_str(&format!("-{line}\n"));
    }
    if no_newline_at_eof {
        diff.push_str(marker);
    }
    for line in &new_lines {
        diff.push_str(&format!("+{line}\n"));
    }
    if no_newline_at_eof {
        diff.push_str(marker);
    }
    let mut mutated_lines: Vec<&str> = lines[..first].to_vec();
    mutated_lines.extend(new_lines.iter().copied());
    mutated_lines.extend(lines[last + 1..].iter().copied());
    let mut mutated = mutated_lines.join("\n");
    if ends_with_newline {
        mutated.push('\n');
    }
    Ok(SyntheticEdit {
        diff,
        mutated,
        first_line: first as u64 + 1,
        last_line: (first + new_lines.len()) as u64,
    })
}

/// The edit a mutant makes: the tool's recorded diff when it has one,
/// otherwise a synthesized one-hunk diff.
fn edit_for(
    path: &str,
    source: &str,
    mutant: &SyntheticMutant,
    profile: &LanguageProfile,
) -> Result<SyntheticEdit, &'static str> {
    match &mutant.recorded_diff {
        Some(diff) => super::cargo_mutants_diff::apply_recorded_diff(path, source, diff),
        None if profile.requires_recorded_diff => Err(RECORDED_DIFF_MISSING),
        None => synthesize_edit(path, source, mutant),
    }
}

/// What `ripr check` said about the mutated lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Exactly one classification and owner on the mutated lines.
    Class {
        token: String,
        owner: String,
        expression: Option<String>,
    },
    NoFinding,
    /// Several findings with different classifications or owners.
    Ambiguous,
    /// The analysis did not complete, so no verdict is read.
    Incomplete,
    /// ripr named a limitation that explains the empty result (for example
    /// `language_adapter_unavailable`); recorded as its own exclusion.
    Limited(&'static str),
}

const LIMITATIONS_THAT_EXPLAIN_NO_FINDING: &[&str] =
    &["language_adapter_unavailable", "language_scope_unsupported"];

/// Read the verdict for lines `first_line..=last_line` of `file` from a
/// `ripr check --format json` report.
pub(crate) fn read_verdict(
    check: &Value,
    file: &str,
    first_line: u64,
    last_line: u64,
    language: &str,
) -> Verdict {
    let matched: Vec<&Value> = check
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| {
            // The Rust adapter reports `./src/lib.rs`; the other adapters
            // report the bare relative path.
            finding
                .pointer("/probe/file")
                .and_then(Value::as_str)
                .map(|probe_file| probe_file.strip_prefix("./").unwrap_or(probe_file))
                == Some(file)
                && finding
                    .pointer("/probe/line")
                    .and_then(Value::as_u64)
                    .is_some_and(|line| (first_line..=last_line).contains(&line))
        })
        .collect();
    let complete = check
        .pointer("/analysis_outcome/analysis_complete")
        .and_then(Value::as_bool)
        == Some(true);
    if matched.is_empty() {
        let limitations = check
            .pointer("/analysis_outcome/outcome/limitations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(reason) = LIMITATIONS_THAT_EXPLAIN_NO_FINDING
            .iter()
            .find(|reason| limitations.contains(**reason))
        {
            return Verdict::Limited(reason);
        }
        return if complete {
            Verdict::NoFinding
        } else {
            Verdict::Incomplete
        };
    }
    if !complete {
        return Verdict::Incomplete;
    }
    // TypeScript and Python name the owner on the probe; the Rust adapter names
    // it on the canonical gap. Two findings on the line with no owner at all
    // cannot be told apart, so they fail closed as ambiguous.
    if matched.len() > 1
        && matched
            .iter()
            .any(|finding| finding_owner(finding).is_empty())
    {
        return Verdict::Ambiguous;
    }
    let verdicts: BTreeSet<(&str, &str)> = matched
        .iter()
        .map(|finding| {
            (
                finding
                    .get("classification")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                finding_owner(finding),
            )
        })
        .collect();
    let mut distinct = verdicts.into_iter();
    match (distinct.next(), distinct.next()) {
        (Some((token, owner)), None) if !token.is_empty() => {
            let has_transitive_reach_limit = matched.iter().any(|finding| {
                finding.get("static_limit_kind").and_then(Value::as_str)
                    == Some("python_transitive_reach_unresolved")
                    || (language == "python"
                        && finding
                            .get("stop_reasons")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .any(|reason| reason.as_str() == Some("transitive_reach_unresolved")))
            });
            if token == "no_static_path" && has_transitive_reach_limit {
                // The producer explicitly could not trace a bounded method
                // path, so this is an honest unknown, not evidence that a
                // caught mutant lacked a discriminator.
                Verdict::Limited("python_transitive_reach_unresolved")
            } else {
                Verdict::Class {
                    token: token.to_string(),
                    owner: owner.to_string(),
                    expression: matched
                        .first()
                        .and_then(|finding| finding.pointer("/probe/expression"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                }
            }
        }
        _ => Verdict::Ambiguous,
    }
}

/// The owner a finding names: on the probe for TypeScript and Python, on the
/// canonical gap for Rust; empty when neither names one.
fn finding_owner(finding: &Value) -> &str {
    finding
        .pointer("/probe/owner")
        .or_else(|| finding.pointer("/canonical_gap/owner"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// The receipt grip class a ripr classification token is tallied under, so
/// the shared recall and agreement tables read the same vocabulary as the
/// Rust path. The raw token is kept in `Record::verdict_token`.
///
/// - `exposed` is `strongly_gripped` (claims a discriminator);
/// - `reachable_unrevealed` (a test reaches the change, no discriminating
///   oracle) and `no_static_path` (no test reaches it) are `ungripped`
///   (claim none exists);
/// - `weakly_exposed` is `weakly_gripped` (no claim one mutant settles);
/// - `infection_unknown` is `activation_unknown`; `propagation_unknown` and
///   `static_unknown` are `opaque` (honest unknowns);
/// - any other token is kept as is and never scored.
pub(crate) fn grip_class_for_token(token: &str) -> String {
    match token {
        "exposed" => "strongly_gripped",
        "reachable_unrevealed" | "no_static_path" => "ungripped",
        "weakly_exposed" => "weakly_gripped",
        "infection_unknown" => "activation_unknown",
        "propagation_unknown" | "static_unknown" => "opaque",
        other => other,
    }
    .to_string()
}

pub(crate) struct Scored {
    pub(crate) records: Vec<Record>,
    /// Files whose report source equals the checkout file.
    pub(crate) sources_matched: usize,
    /// Mutants that passed every pre-check and could be run through ripr.
    pub(crate) eligible: usize,
    /// How many of those were run (an even-stride sample when over the cap).
    pub(crate) selected: usize,
}

/// Why a check run produced no JSON.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CheckError {
    /// This one run failed (ripr exited nonzero, timed out, bad JSON); the
    /// scratch tree is intact, so the mutant is excluded and scoring goes on.
    Failed(String),
    /// The scratch tree may be poisoned (a mutated file could not be written
    /// or the original could not be restored): the whole repo run aborts.
    Fatal(String),
}

/// Indices (0-based, ascending) of the `limit` eligible mutants to run out of
/// `eligible`: every one when it fits, otherwise an even deterministic
/// stride, so the sample spans every file instead of the first few.
pub(crate) fn even_stride_sample(eligible: usize, limit: usize) -> BTreeSet<usize> {
    if eligible <= limit {
        return (0..eligible).collect();
    }
    (0..limit).map(|k| k * eligible / limit).collect()
}

/// Write `mutated` over `target`, run `check`, and put `original` back. A
/// failure to write the mutation (it may be half written) or to restore the
/// original is fatal; a failed check is not.
pub(crate) fn mutate_check_restore(
    target: &Path,
    original: &str,
    mutated: &str,
    check: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, CheckError> {
    if let Err(err) = fs::write(target, mutated) {
        let restore = match fs::write(target, original) {
            Ok(()) => "restored".to_string(),
            Err(restore_err) => format!("restore also failed: {restore_err}"),
        };
        return Err(CheckError::Fatal(format!(
            "write mutated {}: {err} ({restore})",
            target.display()
        )));
    }
    let checked = check();
    fs::write(target, original)
        .map_err(|err| CheckError::Fatal(format!("restore {}: {err}", target.display())))?;
    checked.map_err(CheckError::Failed)
}

fn record(
    file: &str,
    mutant: &SyntheticMutant,
    disposition: &str,
    verdict: Option<(&str, &str, Option<String>)>,
) -> Record {
    let (token, owner, expression) = verdict.unwrap_or(("", "", None));
    Record {
        disposition: disposition.to_string(),
        join_method: AGREEMENT_METHOD.to_string(),
        grip_class: if token.is_empty() {
            String::new()
        } else {
            grip_class_for_token(token)
        },
        outcome: mutant.outcome.clone(),
        seam_id: owner.to_string(),
        seam_kind: if token.is_empty() {
            String::new()
        } else {
            SYNTHETIC_SEAM_KIND.to_string()
        },
        file: file.to_string(),
        line: mutant.start_line,
        expression,
        mutant: format!(
            "{file}:{}:{}: {} [{}] id={}",
            mutant.start_line,
            mutant.start_col + 1,
            mutant.mutator,
            mutant.status,
            mutant.id
        ),
        operator_token: "not_applicable",
        verdict_token: token.to_string(),
    }
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            extensions
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(ext))
        })
}

/// Give every mutant exactly one disposition. `read_checkout` returns the
/// checkout's text for a report path; `run_check` runs ripr for one synthetic
/// edit and returns its JSON report. When more than `max_mutants` mutants are
/// eligible, an even-stride sample of them is run and the rest are
/// `over_mutant_cap`. A [`CheckError::Fatal`] aborts the whole run.
pub(crate) fn score_mutants(
    profile: &LanguageProfile,
    sources: &[MutationSource],
    read_checkout: &dyn Fn(&str) -> Option<String>,
    run_check: &mut dyn FnMut(&str, &SyntheticEdit) -> Result<Value, CheckError>,
    max_mutants: usize,
) -> Result<Scored, String> {
    // Pass 1: the disposition every mutant gets before ripr runs.
    let mut sources_matched = 0;
    let mut pre: Vec<Vec<Option<&'static str>>> = Vec::new();
    for source in sources {
        let file = source.path.as_str();
        let matches = read_checkout(file).is_some_and(|text| text == source.source);
        if matches {
            sources_matched += 1;
        }
        pre.push(
            source
                .mutants
                .iter()
                .map(|mutant| {
                    if !has_extension(file, profile.extensions) {
                        Some(NON_SOURCE_FILE)
                    } else if !matches {
                        Some(SOURCE_MISMATCH)
                    } else if !SCOREABLE_OUTCOMES.contains(&mutant.outcome.as_str()) {
                        Some(UNSCOREABLE_OUTCOME)
                    } else {
                        edit_for(file, &source.source, mutant, profile).err()
                    }
                })
                .collect(),
        );
    }
    let eligible = pre
        .iter()
        .flatten()
        .filter(|reason| reason.is_none())
        .count();
    let chosen = even_stride_sample(eligible, max_mutants);
    // Pass 2: run the chosen ones in report order.
    let mut records = Vec::new();
    let mut ordinal = 0usize;
    for (source, reasons) in sources.iter().zip(&pre) {
        let file = source.path.as_str();
        for (mutant, reason) in source.mutants.iter().zip(reasons) {
            let excluded = |reason: &str| record(file, mutant, reason, None);
            if let Some(reason) = reason {
                records.push(excluded(reason));
                continue;
            }
            let this = ordinal;
            ordinal += 1;
            if !chosen.contains(&this) {
                records.push(excluded(OVER_CAP));
                continue;
            }
            let edit = match edit_for(file, &source.source, mutant, profile) {
                Ok(edit) => edit,
                Err(reason) => {
                    records.push(excluded(reason));
                    continue;
                }
            };
            let check = match run_check(file, &edit) {
                Ok(check) => check,
                Err(CheckError::Failed(_)) => {
                    records.push(excluded(CHECK_FAILED));
                    continue;
                }
                Err(CheckError::Fatal(message)) => {
                    return Err(format!("aborting `{file}` scoring: {message}"));
                }
            };
            records.push(
                match read_verdict(
                    &check,
                    file,
                    edit.first_line,
                    edit.last_line,
                    profile.language,
                ) {
                    Verdict::Class {
                        token,
                        owner,
                        expression,
                    } => record(
                        file,
                        mutant,
                        CANONICAL_PRECISE,
                        Some((&token, &owner, expression)),
                    ),
                    Verdict::NoFinding => excluded(NO_FINDING),
                    Verdict::Ambiguous => excluded(AMBIGUOUS_VERDICT),
                    Verdict::Incomplete => excluded(ANALYSIS_INCOMPLETE),
                    Verdict::Limited(reason) => excluded(reason),
                },
            );
        }
    }
    Ok(Scored {
        records,
        sources_matched,
        eligible,
        selected: chosen.len(),
    })
}

/// Digest of the sorted mutant identities (file, range, mutator, replacement),
/// so two reports over different mutant selections never read as one
/// population. Tool-assigned ids are per-run and are not part of it.
pub(crate) fn mutant_set_sha256(sources: &[MutationSource]) -> String {
    let mut keys: Vec<String> = sources
        .iter()
        .flat_map(|source| {
            source.mutants.iter().map(|m| {
                format!(
                    "{}\t{}:{}-{}:{}\t{}\t{}",
                    source.path,
                    m.start_line,
                    m.start_col,
                    m.end_line,
                    m.end_col,
                    m.mutator,
                    m.replacement
                )
            })
        })
        .collect();
    keys.sort_unstable();
    let mut digest = Sha256::new();
    for key in keys {
        digest.update(key.as_bytes());
        digest.update(b"\n");
    }
    format!("{:x}", digest.finalize())
}

/// A report path is joined under the checkout only when it stays inside it.
pub(crate) fn checkout_relative(path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    candidate
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
        .then(|| candidate.to_path_buf())
}

fn copy_tree(from: &Path, to: &Path, skip_dirs: &[&str]) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| format!("create {}: {err}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|err| format!("read {}: {err}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read {}: {err}", from.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("stat {}: {err}", entry.path().display()))?;
        let name = entry.file_name();
        let target = to.join(&name);
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = name.to_string_lossy();
            if COPY_SKIP_DIRS.contains(&name.as_ref()) || skip_dirs.contains(&name.as_ref()) {
                continue;
            }
            // `skip_dirs` names root-level build output only.
            copy_tree(&entry.path(), &target, &[])?;
        } else {
            fs::copy(entry.path(), &target)
                .map_err(|err| format!("copy {}: {err}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// The scratch tree's `ripr.toml`: `ripr check` enables only Rust unless
/// told otherwise (or a marker file is found), so the language under test is
/// enabled explicitly. It replaces any `ripr.toml` the checkout carries.
pub(crate) fn scratch_ripr_toml(profile: &LanguageProfile) -> String {
    if profile.language == RUST.language {
        return "[languages]\nenabled = [\"rust\"]\n".to_string();
    }
    format!(
        "[languages]\nenabled = [\"rust\", \"{}\"]\n",
        profile.language
    )
}

/// Everything an I/O run over one repo needs.
pub(crate) struct SyntheticRepo<'a> {
    pub(crate) binary: &'a Path,
    pub(crate) scratch: &'a Path,
    pub(crate) name: &'a str,
    pub(crate) checkout: &'a Path,
    pub(crate) profile: LanguageProfile,
    pub(crate) sources: &'a [MutationSource],
    /// Mutation tool identity for the receipt (`name`, `version`, ...).
    pub(crate) tool: Value,
    pub(crate) max_mutants: usize,
}

pub(crate) fn spot_check_synthetic_repo(repo: &SyntheticRepo<'_>) -> Result<RepoRun, String> {
    let SyntheticRepo {
        binary,
        scratch,
        name,
        checkout,
        profile,
        sources,
        tool,
        max_mutants,
    } = repo;
    let revision = run_text(
        "git",
        &[
            "-C".to_string(),
            path_arg(checkout),
            "rev-parse".to_string(),
            "HEAD".to_string(),
        ],
        Duration::from_secs(30),
        "git rev-parse for spot-check checkout",
    )?
    .trim()
    .to_string();
    let tree = scratch.join(format!("{name}.{}-scratch", profile.language));
    if tree.exists() {
        fs::remove_dir_all(&tree).map_err(|err| format!("clear {}: {err}", tree.display()))?;
    }
    copy_tree(checkout, &tree, profile.skip_dirs)?;
    // Rust is enabled by default, so a Rust run keeps the checkout's own
    // ripr.toml (and its oracle overrides) and matches the developer's check.
    let keep_checkout_config =
        profile.language == RUST.language && tree.join("ripr.toml").is_file();
    if !keep_checkout_config {
        fs::write(tree.join("ripr.toml"), scratch_ripr_toml(profile))
            .map_err(|err| format!("write scratch ripr.toml: {err}"))?;
    }
    let diff_path = scratch.join(format!("{name}.synthetic.diff"));

    let read_checkout = |path: &str| {
        let relative = checkout_relative(path)?;
        fs::read_to_string(checkout.join(relative)).ok()
    };
    let mut run_check = |file: &str, edit: &SyntheticEdit| -> Result<Value, CheckError> {
        let relative = checkout_relative(file)
            .ok_or_else(|| CheckError::Failed("report path leaves the checkout".to_string()))?;
        let target = tree.join(relative);
        let original = fs::read_to_string(checkout.join(file))
            .map_err(|err| CheckError::Failed(format!("read {file}: {err}")))?;
        fs::write(&diff_path, &edit.diff)
            .map_err(|err| CheckError::Fatal(format!("write synthetic diff: {err}")))?;
        mutate_check_restore(&target, &original, &edit.mutated, || {
            let output = capture_output_with_timeout(
                &path_arg(binary),
                &[
                    "check".to_string(),
                    "--diff".to_string(),
                    path_arg(&diff_path),
                    "--root".to_string(),
                    path_arg(&tree),
                    "--format".to_string(),
                    "json".to_string(),
                    "--quiet".to_string(),
                ],
                &[],
                CHECK_TIMEOUT,
                "ripr check for synthetic mutant diff",
            )?;
            if output.timed_out || !output.status.is_some_and(|status| status.success()) {
                return Err(format!("ripr check failed: {}", output.stderr.trim()));
            }
            serde_json::from_str(&output.stdout)
                .map_err(|err| format!("parse ripr check JSON: {err}"))
        })
    };
    let scored = score_mutants(
        profile,
        sources,
        &read_checkout,
        &mut run_check,
        *max_mutants,
    )
    .map_err(|err| format!("`{name}`: {err}"))?;
    let mutants_total = scored.records.len();
    Ok(RepoRun {
        name: (*name).to_string(),
        language: profile.language.to_string(),
        revision,
        diffs_checked: scored.sources_matched,
        exposure_run_status: None,
        cargo_mutants_args: None,
        cargo_mutants_version: None,
        mutant_timeout_secs: None,
        mutant_set_sha256: mutant_set_sha256(sources),
        metrics: json!({"mutants_total": mutants_total}),
        records: scored.records,
        pilot: Err("ripr pilot is not run for synthetic-diff repos".to_string()),
        synthetic: Some(json!({
            "agreement_method": AGREEMENT_METHOD,
            "mutation_tool": tool,
            "sampling": {
                "method": "even_stride",
                "max_mutants": max_mutants,
                "eligible": scored.eligible,
                "selected": scored.selected,
                "note": "when eligible exceeds max_mutants, eligible mutants k*eligible/max_mutants for k in 0..max_mutants run, in sorted report order; the rest are over_mutant_cap",
            },
            "files_checked_against_revision": scored.sources_matched,
            "scratch_ripr_toml": scratch_ripr_toml(profile),
            "verdict_grip_class_map": {
                "exposed": "strongly_gripped",
                "reachable_unrevealed": "ungripped",
                "no_static_path": "ungripped",
                "weakly_exposed": "weakly_gripped",
                "infection_unknown": "activation_unknown",
                "propagation_unknown": "opaque",
                "static_unknown": "opaque",
            },
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::mutation_spot_check::{agreement, verdict_family};

    fn claim_family(record: &Record) -> &'static str {
        verdict_family(&record.grip_class)
    }

    fn mutant(
        id: &str,
        start: (u64, u64),
        end: (u64, u64),
        replacement: &str,
        outcome: &str,
    ) -> SyntheticMutant {
        SyntheticMutant {
            id: id.to_string(),
            mutator: "EqualityOperator".to_string(),
            replacement: replacement.to_string(),
            start_line: start.0,
            start_col: start.1,
            end_line: end.0,
            end_col: end.1,
            status: outcome.to_string(),
            outcome: outcome.to_string(),
            recorded_diff: None,
        }
    }

    const SOURCE: &str = "export function discount(total: number, qty: number): number {\n  if (qty >= 10) {\n    return total * 0.9;\n  }\n  return total;\n}\n";

    #[test]
    fn single_line_edit_builds_a_one_hunk_diff_and_the_mutated_file() -> Result<(), String> {
        // `qty >= 10` spans columns 6..15 of line 2; replace the whole condition.
        let edit = synthesize_edit(
            "src/price.ts",
            SOURCE,
            &mutant("1", (2, 6), (2, 15), "qty > 10", "caught"),
        )
        .map_err(str::to_string)?;
        assert_eq!(
            edit.diff,
            "diff --git a/src/price.ts b/src/price.ts\n--- a/src/price.ts\n+++ b/src/price.ts\n@@ -2,1 +2,1 @@\n-  if (qty >= 10) {\n+  if (qty > 10) {\n"
        );
        assert_eq!((edit.first_line, edit.last_line), (2, 2));
        assert_eq!(edit.mutated, SOURCE.replace("qty >= 10", "qty > 10"));
        Ok(())
    }

    #[test]
    fn multi_line_range_and_multi_line_replacement_are_one_hunk() -> Result<(), String> {
        // Replace `{\n    return total * 0.9;\n  }` (line 2 col 17 .. line 4 col 3) with `{}`.
        let edit = synthesize_edit(
            "a.ts",
            SOURCE,
            &mutant("2", (2, 17), (4, 3), "{}", "missed"),
        )
        .map_err(str::to_string)?;
        assert!(edit.diff.contains("@@ -2,3 +2,1 @@\n"));
        assert!(edit.diff.contains("-    return total * 0.9;\n"));
        assert!(edit.diff.contains("+  if (qty >= 10) {}\n"));
        assert_eq!((edit.first_line, edit.last_line), (2, 2));
        assert_eq!(
            edit.mutated,
            "export function discount(total: number, qty: number): number {\n  if (qty >= 10) {}\n  return total;\n}\n"
        );
        let growing = synthesize_edit(
            "a.ts",
            SOURCE,
            &mutant("3", (5, 9), (5, 14), "a\nb", "missed"),
        )
        .map_err(str::to_string)?;
        assert!(growing.diff.contains("@@ -5,1 +5,2 @@\n"));
        assert_eq!((growing.first_line, growing.last_line), (5, 6));
        Ok(())
    }

    #[test]
    fn columns_count_utf16_units_and_reject_surrogate_splits() -> Result<(), String> {
        // The emoji is two UTF-16 units: `x` starts at column 5.
        let source = "const a = \"\u{1F600}\" + x;\n";
        let prefix = "const a = \"\u{1F600}\" + ";
        let col = prefix.encode_utf16().count() as u64;
        let edit = synthesize_edit(
            "a.ts",
            source,
            &mutant("1", (1, col), (1, col + 1), "y", "caught"),
        )
        .map_err(str::to_string)?;
        assert_eq!(edit.mutated, "const a = \"\u{1F600}\" + y;\n");
        // Column 12 is between the two units of the emoji.
        let split = synthesize_edit(
            "a.ts",
            source,
            &mutant("2", (1, 11), (1, 12), "y", "caught"),
        );
        assert_eq!(split, Err(INVALID_LOCATION));
        Ok(())
    }

    #[test]
    fn missing_final_newline_is_marked_on_both_sides() -> Result<(), String> {
        let edit = synthesize_edit(
            "a.ts",
            "a >= b",
            &mutant("1", (1, 2), (1, 4), ">", "caught"),
        )
        .map_err(str::to_string)?;
        assert_eq!(
            edit.diff,
            "diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1,1 +1,1 @@\n-a >= b\n\\ No newline at end of file\n+a > b\n\\ No newline at end of file\n"
        );
        assert_eq!(edit.mutated, "a > b");
        Ok(())
    }

    #[test]
    fn noop_and_out_of_range_mutants_are_refused_with_a_reason() {
        let noop = synthesize_edit(
            "a.ts",
            SOURCE,
            &mutant("1", (2, 6), (2, 15), "qty >= 10", "caught"),
        );
        assert_eq!(noop, Err(NOOP_MUTANT));
        let past_end = synthesize_edit(
            "a.ts",
            SOURCE,
            &mutant("2", (99, 0), (99, 1), "x", "caught"),
        );
        assert_eq!(past_end, Err(INVALID_LOCATION));
        let zero_line =
            synthesize_edit("a.ts", SOURCE, &mutant("3", (0, 0), (0, 1), "x", "caught"));
        assert_eq!(zero_line, Err(INVALID_LOCATION));
        let backwards =
            synthesize_edit("a.ts", SOURCE, &mutant("4", (2, 9), (2, 6), "x", "caught"));
        assert_eq!(backwards, Err(INVALID_LOCATION));
        let col_past_line = synthesize_edit(
            "a.ts",
            SOURCE,
            &mutant("5", (2, 6), (2, 400), "x", "caught"),
        );
        assert_eq!(col_past_line, Err(INVALID_LOCATION));
    }

    #[test]
    fn synthesized_diff_matches_a_first_party_project_on_disk() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-synthetic-diff-{}-{}",
            std::process::id(),
            "projects"
        ));
        let src = dir.join("src");
        fs::create_dir_all(&src).map_err(|err| err.to_string())?;
        let file = src.join("price.ts");
        fs::write(&file, SOURCE).map_err(|err| err.to_string())?;
        let on_disk = fs::read_to_string(&file).map_err(|err| err.to_string())?;
        let edit = synthesize_edit(
            "src/price.ts",
            &on_disk,
            &mutant("1", (2, 6), (2, 15), "qty > 10", "caught"),
        )
        .map_err(str::to_string)?;
        // Applying the hunk's `-`/`+` lines to the file reproduces `mutated`.
        let removed: Vec<&str> = edit
            .diff
            .lines()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .map(|line| &line[1..])
            .collect();
        let added: Vec<&str> = edit
            .diff
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .map(|line| &line[1..])
            .collect();
        let mut applied = on_disk.clone();
        applied = applied.replacen(
            &format!("{}\n", removed.join("\n")),
            &format!("{}\n", added.join("\n")),
            1,
        );
        assert_eq!(applied, edit.mutated);
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(())
    }

    fn check_json(
        findings: &[(&str, &str, u64, &str)],
        complete: bool,
        limitation: Option<&str>,
    ) -> Value {
        json!({
            "analysis_outcome": {
                "analysis_complete": complete,
                "outcome": {"limitations": limitation.map_or_else(Vec::new, |kind| vec![json!({"kind": kind})])},
            },
            "findings": findings.iter().map(|(class, file, line, owner)| json!({
                "classification": class,
                "probe": {"file": file, "line": line, "owner": owner, "expression": "  if (qty > 10) {"},
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn verdict_reads_one_class_on_the_mutated_lines_only() {
        let check = check_json(
            &[
                ("exposed", "a.ts", 2, "typescript:a.ts::f"),
                ("reachable_unrevealed", "a.ts", 30, "typescript:a.ts::g"),
                ("no_static_path", "b.ts", 2, "typescript:b.ts::h"),
            ],
            true,
            None,
        );
        assert_eq!(
            read_verdict(&check, "a.ts", 2, 2, "typescript"),
            Verdict::Class {
                token: "exposed".to_string(),
                owner: "typescript:a.ts::f".to_string(),
                expression: Some("  if (qty > 10) {".to_string()),
            }
        );
        assert_eq!(
            read_verdict(&check, "a.ts", 5, 6, "typescript"),
            Verdict::NoFinding
        );
    }

    #[test]
    fn verdict_matches_a_dot_slash_probe_path() {
        // `ripr check` on Rust reports `./src/lib.rs`, not `src/lib.rs`.
        let check = check_json(
            &[("weakly_exposed", "./src/lib.rs", 2, "rust:src/lib.rs::f")],
            true,
            None,
        );
        assert!(matches!(
            read_verdict(&check, "src/lib.rs", 2, 2, "rust"),
            Verdict::Class { ref token, .. } if token == "weakly_exposed"
        ));
        assert_eq!(
            read_verdict(&check, "lib.rs", 2, 2, "rust"),
            Verdict::NoFinding
        );
    }

    fn rust_check_json(findings: &[(&str, u64, Option<&str>)]) -> Value {
        // The Rust adapter names the owner on `canonical_gap`, not on `probe`.
        json!({
            "analysis_outcome": {"analysis_complete": true, "outcome": {"limitations": []}},
            "findings": findings.iter().map(|(class, line, owner)| json!({
                "classification": class,
                "probe": {"file": "./src/lib.rs", "line": line, "expression": "a > 1"},
                "canonical_gap": owner.map_or(json!({}), |owner| json!({"owner": owner})),
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn rust_owner_comes_from_the_canonical_gap_and_unowned_pairs_fail_closed() {
        let distinct = rust_check_json(&[("exposed", 2, Some("f")), ("exposed", 3, Some("g"))]);
        assert_eq!(
            read_verdict(&distinct, "src/lib.rs", 2, 3, "rust"),
            Verdict::Ambiguous
        );
        let one = rust_check_json(&[("exposed", 2, Some("f"))]);
        assert!(matches!(
            read_verdict(&one, "src/lib.rs", 2, 2, "rust"),
            Verdict::Class { ref owner, .. } if owner == "f"
        ));
        let unowned = rust_check_json(&[("exposed", 2, None), ("exposed", 3, None)]);
        assert_eq!(
            read_verdict(&unowned, "src/lib.rs", 2, 3, "rust"),
            Verdict::Ambiguous
        );
    }

    #[test]
    fn verdict_is_ambiguous_for_mixed_classes_or_owners_and_agrees_for_duplicates() {
        let mixed = check_json(
            &[
                ("exposed", "a.ts", 2, "o"),
                ("weakly_exposed", "a.ts", 3, "o"),
            ],
            true,
            None,
        );
        assert_eq!(
            read_verdict(&mixed, "a.ts", 2, 3, "typescript"),
            Verdict::Ambiguous
        );
        let owners = check_json(
            &[("exposed", "a.ts", 2, "o1"), ("exposed", "a.ts", 3, "o2")],
            true,
            None,
        );
        assert_eq!(
            read_verdict(&owners, "a.ts", 2, 3, "typescript"),
            Verdict::Ambiguous
        );
        let same = check_json(
            &[("exposed", "a.ts", 2, "o"), ("exposed", "a.ts", 3, "o")],
            true,
            None,
        );
        assert!(matches!(
            read_verdict(&same, "a.ts", 2, 3, "typescript"),
            Verdict::Class { .. }
        ));
    }

    #[test]
    fn verdict_names_adapter_limitations_and_incomplete_runs() {
        let limited = check_json(&[], true, Some("language_adapter_unavailable"));
        assert_eq!(
            read_verdict(&limited, "a.ts", 2, 2, "typescript"),
            Verdict::Limited("language_adapter_unavailable")
        );
        let scope = check_json(&[], true, Some("language_scope_unsupported"));
        assert_eq!(
            read_verdict(&scope, "a.ts", 2, 2, "typescript"),
            Verdict::Limited("language_scope_unsupported")
        );
        let incomplete = check_json(&[], false, None);
        assert_eq!(
            read_verdict(&incomplete, "a.ts", 2, 2, "typescript"),
            Verdict::Incomplete
        );
        let partial = check_json(&[("exposed", "a.ts", 2, "o")], false, None);
        assert_eq!(
            read_verdict(&partial, "a.ts", 2, 2, "typescript"),
            Verdict::Incomplete
        );
    }

    #[test]
    fn python_transitive_reach_limits_are_honest_unknowns_not_false_gaps() {
        let mut by_kind = check_json(&[("no_static_path", "a.py", 2, "o")], true, None);
        by_kind["findings"][0]["static_limit_kind"] = json!("python_transitive_reach_unresolved");
        assert_eq!(
            read_verdict(&by_kind, "a.py", 2, 2, "python"),
            Verdict::Limited("python_transitive_reach_unresolved")
        );

        let mut by_stop_reason = check_json(&[("no_static_path", "a.py", 2, "o")], true, None);
        by_stop_reason["findings"][0]["stop_reasons"] = json!(["transitive_reach_unresolved"]);
        assert_eq!(
            read_verdict(&by_stop_reason, "a.py", 2, 2, "python"),
            Verdict::Limited("python_transitive_reach_unresolved")
        );

        let unlimited = check_json(&[("no_static_path", "a.py", 2, "o")], true, None);
        assert!(matches!(
            read_verdict(&unlimited, "a.py", 2, 2, "python"),
            Verdict::Class { .. }
        ));
    }

    #[test]
    fn rust_transitive_reach_keeps_its_own_verdict_and_language_identity() {
        let mut rust = check_json(&[("no_static_path", "src/lib.rs", 2, "o")], true, None);
        rust["findings"][0]["static_limit_kind"] = json!("rust_transitive_reach_unresolved");
        rust["findings"][0]["stop_reasons"] = json!(["transitive_reach_unresolved"]);
        assert!(matches!(
            read_verdict(&rust, "src/lib.rs", 2, 2, "rust"),
            Verdict::Class { .. }
        ));
    }

    /// #14083 T9: the verdict join reads a real `ripr check --diff` JSON
    /// document, not only hand-built ones. The hand-built Rust documents fix
    /// the field names by assumption (`./`-prefixed probe path, the owner on
    /// `canonical_gap`, `analysis_outcome.analysis_complete`); a renamed or
    /// moved field would keep them green while every real mutant read as
    /// `NoFinding` or `Incomplete`.
    #[test]
    fn verdict_reads_a_real_rust_check_document() -> Result<(), String> {
        const LIB: &str = "pub fn over_limit(amount: u32) -> bool {\n    amount > 10\n}\n";
        const TEST: &str = "use real_check::over_limit;\n\n#[test]\nfn boundary() {\n    assert_eq!(over_limit(10), false);\n    assert_eq!(over_limit(11), true);\n}\n";
        let root = std::env::temp_dir().join(format!(
            "ripr-synthetic-diff-{}-real-rust-check",
            std::process::id()
        ));
        let write = |relative: &str, text: &str| -> Result<(), String> {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            fs::write(&path, text).map_err(|err| format!("write {relative}: {err}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"real_check\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("tests/boundary.rs", TEST)?;
        // The mutant is applied to the file and described by a diff, as the
        // diff-mode scorer does for each cargo-mutants mutant.
        let edit = synthesize_edit(
            "src/lib.rs",
            LIB,
            &mutant("1", (2, 4), (2, 15), "amount >= 10", "caught"),
        )
        .map_err(str::to_string)?;
        write("src/lib.rs", &edit.mutated)?;
        write("mutant.diff", &edit.diff)?;
        let output = ripr::check_workspace(ripr::CheckInput {
            root: root.clone(),
            base: None,
            diff_file: Some(root.join("mutant.diff")),
            mode: ripr::Mode::Ready,
            format: ripr::OutputFormat::Json,
            include_unchanged_tests: true,
            ..ripr::CheckInput::default()
        });
        let rendered =
            output.and_then(|output| ripr::render_check(&output, &ripr::OutputFormat::Json));
        fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
        let check: Value = serde_json::from_str(&rendered?).map_err(|err| err.to_string())?;
        let verdict = read_verdict(&check, "src/lib.rs", 2, 2);
        let Verdict::Class { token, owner, .. } = &verdict else {
            return Err(format!(
                "expected one class on the mutated line, got {verdict:?}: {check}"
            ));
        };
        if token != "exposed" || !owner.contains("over_limit") {
            return Err(format!("unexpected verdict {verdict:?}: {check}"));
        }
        // The same real document has nothing on an untouched line.
        assert_eq!(read_verdict(&check, "src/lib.rs", 1, 1), Verdict::NoFinding);
        Ok(())
    }

    #[test]
    fn tokens_map_to_the_shared_grip_class_vocabulary() {
        for (token, class, family) in [
            ("exposed", "strongly_gripped", "claims_discriminator"),
            (
                "reachable_unrevealed",
                "ungripped",
                "claims_no_discriminator",
            ),
            ("no_static_path", "ungripped", "claims_no_discriminator"),
            ("weakly_exposed", "weakly_gripped", "weak"),
            ("infection_unknown", "activation_unknown", "unknown"),
            ("propagation_unknown", "opaque", "unknown"),
            ("static_unknown", "opaque", "unknown"),
            ("brand_new_token", "brand_new_token", "unknown"),
        ] {
            assert_eq!(grip_class_for_token(token), class, "{token}");
            assert_eq!(verdict_family(class), family, "{token}");
        }
    }

    fn source(path: &str, text: &str, mutants: Vec<SyntheticMutant>) -> MutationSource {
        MutationSource {
            path: path.to_string(),
            source: text.to_string(),
            mutants,
        }
    }

    #[test]
    fn every_mutant_gets_one_disposition_and_scoring_follows_the_claim() -> Result<(), String> {
        let other = "const y = 1;\n";
        let sources = vec![
            source(
                "src/price.ts",
                SOURCE,
                vec![
                    // exposed + caught: agree
                    mutant("1", (2, 6), (2, 15), "qty > 10", "caught"),
                    // exposed + missed: overclaim
                    mutant("2", (2, 6), (2, 15), "qty > 11", "missed"),
                    // timeout: never scored, never run
                    mutant("3", (2, 6), (2, 15), "qty < 10", "timeout"),
                    // noop
                    mutant("4", (2, 6), (2, 15), "qty >= 10", "caught"),
                    // no finding on line 5
                    mutant("5", (5, 9), (5, 14), "0", "missed"),
                    // out of range
                    mutant("6", (40, 0), (40, 1), "0", "missed"),
                ],
            ),
            // checkout text differs from the report's source
            source(
                "src/stale.ts",
                "old\n",
                vec![mutant("7", (1, 0), (1, 3), "new", "caught")],
            ),
            source(
                "README.md",
                "# x\n",
                vec![mutant("8", (1, 2), (1, 3), "y", "caught")],
            ),
            source(
                "src/other.ts",
                other,
                vec![
                    // reachable_unrevealed + missed: agree
                    mutant("9", (1, 10), (1, 11), "2", "missed"),
                    // reachable_unrevealed + caught: false gap
                    mutant("10", (1, 10), (1, 11), "3", "caught"),
                    // ripr failure
                    mutant("11", (1, 10), (1, 11), "4", "caught"),
                    // over the cap of 5 runs
                    mutant("12", (1, 10), (1, 11), "5", "caught"),
                ],
            ),
        ];
        let read = |path: &str| match path {
            "src/price.ts" => Some(SOURCE.to_string()),
            "src/stale.ts" => Some("changed\n".to_string()),
            "src/other.ts" => Some(other.to_string()),
            _ => None,
        };
        let mut calls = 0;
        let mut run = |file: &str, edit: &SyntheticEdit| -> Result<Value, CheckError> {
            calls += 1;
            if edit.diff.contains("+const y = 4;") {
                return Err(CheckError::Failed("boom".to_string()));
            }
            if file == "src/other.ts" {
                return Ok(check_json(
                    &[(
                        "reachable_unrevealed",
                        "src/other.ts",
                        1,
                        "typescript:src/other.ts::f",
                    )],
                    true,
                    None,
                ));
            }
            Ok(check_json(
                &[(
                    "exposed",
                    "src/price.ts",
                    2,
                    "typescript:src/price.ts::discount",
                )],
                true,
                None,
            ))
        };
        let scored = score_mutants(&TYPESCRIPT, &sources, &read, &mut run, 5)?;
        let total: usize = sources.iter().map(|s| s.mutants.len()).sum();
        assert_eq!(scored.records.len(), total);
        assert_eq!(scored.sources_matched, 2);
        // Eligible: ids 1, 2, 5, 9, 10, 11, 12 (7). An even stride of 5 runs
        // ordinals 0, 1, 2, 4, 5: ids 1, 2, 5, 10, 11. Ids 9 and 12 are over the cap.
        assert_eq!((scored.eligible, scored.selected), (7, 5));
        let by_id = |id: &str| {
            scored
                .records
                .iter()
                .find(|record| record.mutant.ends_with(&format!("id={id}")))
                .ok_or_else(|| format!("no record for {id}"))
        };
        let disposition = |id: &str| by_id(id).map(|record| record.disposition.clone());
        assert_eq!(disposition("1")?, CANONICAL_PRECISE);
        assert_eq!(disposition("2")?, CANONICAL_PRECISE);
        assert_eq!(disposition("3")?, UNSCOREABLE_OUTCOME);
        assert_eq!(disposition("4")?, NOOP_MUTANT);
        assert_eq!(disposition("5")?, NO_FINDING);
        assert_eq!(disposition("6")?, INVALID_LOCATION);
        assert_eq!(disposition("7")?, SOURCE_MISMATCH);
        assert_eq!(disposition("8")?, NON_SOURCE_FILE);
        assert_eq!(disposition("9")?, OVER_CAP);
        assert_eq!(disposition("10")?, CANONICAL_PRECISE);
        assert_eq!(disposition("11")?, CHECK_FAILED);
        assert_eq!(disposition("12")?, OVER_CAP);
        assert_eq!(calls, 5);

        let verdict =
            |id: &str| by_id(id).map(|record| agreement(claim_family(record), &record.outcome));
        assert_eq!(verdict("1")?, "agree");
        assert_eq!(verdict("2")?, "overclaim");
        assert_eq!(verdict("10")?, "false_gap");
        assert_eq!(by_id("1")?.verdict_token, "exposed");
        assert_eq!(by_id("10")?.verdict_token, "reachable_unrevealed");
        assert_eq!(by_id("10")?.join_method, AGREEMENT_METHOD);
        Ok(())
    }

    #[test]
    fn ripr_failures_and_adapter_limits_are_exclusions_not_scores() -> Result<(), String> {
        let sources = vec![source(
            "a.ts",
            SOURCE,
            vec![
                mutant("1", (2, 6), (2, 15), "qty > 10", "caught"),
                mutant("2", (2, 6), (2, 15), "qty > 11", "caught"),
                mutant("3", (2, 6), (2, 15), "qty > 12", "caught"),
            ],
        )];
        let read = |_: &str| Some(SOURCE.to_string());
        let mut n = 0;
        let mut run = |_: &str, _: &SyntheticEdit| -> Result<Value, CheckError> {
            n += 1;
            match n {
                1 => Err(CheckError::Failed("exit 2".to_string())),
                2 => Ok(check_json(&[], true, Some("language_adapter_unavailable"))),
                _ => Ok(check_json(&[], false, None)),
            }
        };
        let scored = score_mutants(&TYPESCRIPT, &sources, &read, &mut run, 10)?;
        let dispositions: Vec<&str> = scored
            .records
            .iter()
            .map(|record| record.disposition.as_str())
            .collect();
        assert_eq!(
            dispositions,
            [
                CHECK_FAILED,
                "language_adapter_unavailable",
                ANALYSIS_INCOMPLETE
            ]
        );
        Ok(())
    }

    #[test]
    fn even_stride_sample_spans_the_whole_list_deterministically() {
        assert_eq!(even_stride_sample(3, 5), BTreeSet::from([0, 1, 2]));
        assert_eq!(even_stride_sample(0, 5), BTreeSet::new());
        assert_eq!(even_stride_sample(10, 5), BTreeSet::from([0, 2, 4, 6, 8]));
        assert_eq!(even_stride_sample(7, 5), BTreeSet::from([0, 1, 2, 4, 5]));
        let big = even_stride_sample(10_000, 1500);
        assert_eq!(big.len(), 1500);
        assert_eq!(big.first(), Some(&0));
        // The last pick lies in the final stride, not the first 1500.
        assert!(big.last().is_some_and(|last| *last > 9_000));
        assert_eq!(big, even_stride_sample(10_000, 1500));
    }

    #[test]
    fn capped_runs_sample_across_files_and_record_the_counts() -> Result<(), String> {
        let make = |path: &str, base: u64| {
            source(
                path,
                SOURCE,
                (0..10)
                    .map(|i| {
                        mutant(
                            &format!("{base}{i}"),
                            (2, 6),
                            (2, 15),
                            &format!("qty > {i}"),
                            "caught",
                        )
                    })
                    .collect(),
            )
        };
        let sources = vec![make("a.ts", 1), make("b.ts", 2)];
        let read = |_: &str| Some(SOURCE.to_string());
        let mut run = |_: &str, _: &SyntheticEdit| -> Result<Value, CheckError> {
            Ok(check_json(&[("exposed", "a.ts", 2, "o")], true, None))
        };
        let scored = score_mutants(&TYPESCRIPT, &sources, &read, &mut run, 4)?;
        assert_eq!((scored.eligible, scored.selected), (20, 4));
        let ran_in = |file: &str| {
            scored
                .records
                .iter()
                .filter(|r| r.file == file && r.disposition != OVER_CAP)
                .count()
        };
        // Both files are sampled, not just the first.
        assert_eq!((ran_in("a.ts"), ran_in("b.ts")), (2, 2));
        assert_eq!(scored.records.len(), 20);
        Ok(())
    }

    #[test]
    fn a_rust_mutant_without_a_recorded_diff_is_excluded_not_rebuilt() {
        let m = mutant("m", (2, 6), (2, 15), "qty < 10", "missed");
        assert_eq!(
            edit_for("src/price.ts", SOURCE, &m, &RUST),
            Err(RECORDED_DIFF_MISSING)
        );
        // Tools that report columns and replacement text still synthesize.
        assert_eq!(
            edit_for("src/price.ts", SOURCE, &m, &TYPESCRIPT).map(|_| ()),
            Ok(())
        );
    }

    #[test]
    fn root_build_dirs_are_skipped_but_a_nested_module_of_the_same_name_is_kept()
    -> Result<(), String> {
        let from = scratch_dir("copy-from")?;
        let to = scratch_dir("copy-to")?;
        let _ = fs::remove_dir_all(&to);
        for dir in ["target/debug", "src/target", "mutants.out"] {
            fs::create_dir_all(from.join(dir)).map_err(|err| err.to_string())?;
        }
        fs::write(from.join("target/debug/x"), "build").map_err(|err| err.to_string())?;
        fs::write(from.join("src/target/mod.rs"), "pub fn f() {}")
            .map_err(|err| err.to_string())?;
        fs::write(from.join("mutants.out/outcomes.json"), "{}").map_err(|err| err.to_string())?;
        copy_tree(&from, &to, RUST.skip_dirs)?;
        let kept = to.join("src/target/mod.rs").exists();
        let root_target = to.join("target").exists();
        let mutants_out = to.join("mutants.out").exists();
        let _ = fs::remove_dir_all(&from);
        let _ = fs::remove_dir_all(&to);
        assert!(kept, "a nested src/target module must be copied");
        assert!(!root_target, "root target/ must be skipped");
        assert!(!mutants_out, "root mutants.out/ must be skipped");
        Ok(())
    }

    fn scratch_dir(tag: &str) -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!("ripr-synthetic-{}-{tag}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(dir)
    }

    #[test]
    fn mutate_check_restore_restores_the_original_on_success_and_on_check_failure()
    -> Result<(), String> {
        let dir = scratch_dir("restore-ok")?;
        let target = dir.join("a.ts");
        fs::write(&target, "orig").map_err(|err| err.to_string())?;
        let seen = mutate_check_restore(&target, "orig", "mutated", || {
            Ok(json!({"saw": fs::read_to_string(&target).unwrap_or_default()}))
        });
        assert_eq!(seen, Ok(json!({"saw": "mutated"})));
        assert_eq!(
            fs::read_to_string(&target).map_err(|e| e.to_string())?,
            "orig"
        );
        let failed = mutate_check_restore(&target, "orig", "mutated", || Err("exit 2".to_string()));
        assert_eq!(failed, Err(CheckError::Failed("exit 2".to_string())));
        assert_eq!(
            fs::read_to_string(&target).map_err(|e| e.to_string())?,
            "orig"
        );
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())
    }

    #[test]
    fn a_failed_mutation_write_or_restore_is_fatal() -> Result<(), String> {
        let dir = scratch_dir("fatal")?;
        // The parent directory does not exist, so the mutation cannot be written.
        let missing = dir.join("no-such-dir").join("a.ts");
        let write_failed = mutate_check_restore(&missing, "orig", "mutated", || Ok(json!({})));
        assert!(matches!(write_failed, Err(CheckError::Fatal(_))));
        // The check replaces the file with a directory, so the restore fails.
        let target = dir.join("b.ts");
        fs::write(&target, "orig").map_err(|err| err.to_string())?;
        let restore_failed = mutate_check_restore(&target, "orig", "mutated", || {
            fs::remove_file(&target).map_err(|err| err.to_string())?;
            fs::create_dir(&target).map_err(|err| err.to_string())?;
            Ok(json!({}))
        });
        assert!(matches!(restore_failed, Err(CheckError::Fatal(_))));
        fs::remove_dir_all(&dir).map_err(|err| err.to_string())
    }

    #[test]
    fn a_fatal_check_error_aborts_scoring_of_later_mutants() {
        let sources = vec![source(
            "a.ts",
            SOURCE,
            vec![
                mutant("1", (2, 6), (2, 15), "qty > 10", "caught"),
                mutant("2", (2, 6), (2, 15), "qty > 11", "caught"),
            ],
        )];
        let read = |_: &str| Some(SOURCE.to_string());
        let mut calls = 0;
        let mut run = |_: &str, _: &SyntheticEdit| -> Result<Value, CheckError> {
            calls += 1;
            Err(CheckError::Fatal("restore failed".to_string()))
        };
        let result = score_mutants(&TYPESCRIPT, &sources, &read, &mut run, 10);
        assert!(
            result
                .err()
                .is_some_and(|err| err.contains("restore failed"))
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn scratch_config_enables_the_language_and_paths_must_stay_inside() {
        assert_eq!(
            scratch_ripr_toml(&TYPESCRIPT),
            "[languages]\nenabled = [\"rust\", \"typescript\"]\n"
        );
        let python = LanguageProfile {
            language: "python",
            extensions: &["py"],
            skip_dirs: &[],
            requires_recorded_diff: false,
        };
        assert!(scratch_ripr_toml(&python).contains("\"python\""));
        assert_eq!(
            scratch_ripr_toml(&RUST),
            "[languages]\nenabled = [\"rust\"]\n"
        );
        assert!(checkout_relative("src/a.ts").is_some());
        assert!(checkout_relative("../a.ts").is_none());
        assert!(checkout_relative("/etc/passwd").is_none());
    }

    #[test]
    fn mutant_set_digest_ignores_tool_ids_and_order() {
        let a = vec![source(
            "a.ts",
            SOURCE,
            vec![
                mutant("1", (2, 6), (2, 15), "x", "caught"),
                mutant("2", (3, 6), (3, 15), "y", "caught"),
            ],
        )];
        let b = vec![source(
            "a.ts",
            SOURCE,
            vec![
                mutant("9", (3, 6), (3, 15), "y", "missed"),
                mutant("8", (2, 6), (2, 15), "x", "caught"),
            ],
        )];
        assert_eq!(mutant_set_sha256(&a), mutant_set_sha256(&b));
        let c = vec![source(
            "a.ts",
            SOURCE,
            vec![mutant("1", (2, 6), (2, 15), "z", "caught")],
        )];
        assert_ne!(mutant_set_sha256(&a), mutant_set_sha256(&c));
    }
}

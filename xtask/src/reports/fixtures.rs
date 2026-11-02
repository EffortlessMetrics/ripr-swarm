//! Goldens + fixture-runner + drift-semantics cluster: the fixture runner
//! (`fixtures` gate, per-fixture `ripr` invocation, golden comparison), the
//! `goldens check` / `goldens bless` machinery, the `golden-drift` report, and
//! the drift-semantics classification that separates semantic drift from
//! formatting-only drift.
//!
//! Extracted verbatim from `main.rs` as a behavior-preserving decomposition
//! slice of #2119. Items are `pub(crate)` and re-exported from `main.rs` so
//! existing call sites (`dispatch.rs`, `dogfood.rs`, `fixture_contracts`,
//! `evidence_promotion`, and `tests.rs`) compile unchanged.

use crate::no_panic::contains_word;
use crate::run::{run_output_bytes, run_output_optional, run_output_owned_with_envs};
use crate::{
    collect_pr_changes, forbidden_static_terms, has_markdown_heading, json_escape, markdown_cell,
    normalize_path, read_text_lossy, ripr_debug_binary, ripr_debug_binary_in,
    write_json_string_array, write_report,
};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod observation_tail_return;
mod preparation;
mod transitional;

/// Result of a single fixture run: violation lines + optional run output.
type FixtureResult = Result<(Vec<String>, Option<FixtureRun>), String>;

pub(crate) fn fixtures_impl(name: Option<&String>) -> Result<(), String> {
    // The build that #2110 placed here now belongs to `ripr_fixture_binary`,
    // which every scenario already goes through and which builds exactly once
    // per process (#3054). Keeping a copy here would leave the invariant
    // looking like an entry-point courtesy rather than a property of reaching
    // the binary at all.
    let fixture_dirs = fixture_dirs()?;
    let selected = match name {
        Some(value) => vec![fixture_dir_for_name(value)?],
        None => fixture_dirs,
    };
    // Parallelize fixture execution via rayon (#2415). Each fixture is
    // independent (disjoint input/expected dirs), so parallel runs produce
    // identical output.
    let results: Vec<FixtureResult> = selected
        .par_iter()
        .map(|path| {
            if !path.exists() {
                return Ok((
                    vec![format!("fixture does not exist: {}", normalize_path(path))],
                    None,
                ));
            }
            if !path.is_dir() {
                return Ok((
                    vec![format!(
                        "fixture is not a directory: {}",
                        normalize_path(path)
                    )],
                    None,
                ));
            }
            let contract_violations = fixture_contract_violations(path)?;
            if !contract_violations.is_empty() {
                return Ok((contract_violations, None));
            }
            match run_fixture(path) {
                Ok(run) => Ok((run.comparison_violations(), Some(run))),
                Err(err) => Err(err),
            }
        })
        .collect();

    let mut violations = Vec::new();
    let mut runs = Vec::new();
    for result in results {
        match result {
            Ok((vios, maybe_run)) => {
                violations.extend(vios);
                if let Some(run) = maybe_run {
                    runs.push(run);
                }
            }
            Err(err) => violations.push(err),
        }
    }

    let body = fixture_report_body(name.map(String::as_str), &selected, &runs, &violations);
    write_report("fixtures.md", &body)?;

    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "fixture command failed; see target/ripr/reports/fixtures.md\n{}",
            violations.join("\n")
        ))
    }
}

pub(crate) fn goldens_impl(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("prepare") => preparation::prepare(&args[1..]),
        Some("check") => goldens_check(),
        Some("bless") => {
            let Some(name) = args.get(1) else {
                return Err(
                    "goldens bless requires a fixture name\nusage: cargo xtask goldens bless <name> --reason <reason>"
                        .to_string(),
                );
            };
            let reason = parse_reason(&args[2..])?;
            goldens_bless(name, &reason)
        }
        Some(other) => Err(format!(
            "unknown goldens command `{other}`\nusage: cargo xtask goldens prepare [--offline]\n       cargo xtask goldens check\n       cargo xtask goldens bless <name> --reason <reason>"
        )),
        None => Err(
            "missing goldens command\nusage: cargo xtask goldens prepare [--offline]\n       cargo xtask goldens check\n       cargo xtask goldens bless <name> --reason <reason>"
                .to_string(),
        ),
    }
}

pub(crate) fn goldens_check() -> Result<(), String> {
    match golden_check_run()? {
        GoldenCheckRun::Clean => Ok(()),
        GoldenCheckRun::Failed { message, .. } => Err(message),
    }
}

/// What a `goldens check` run established, before the caller decides how to
/// treat violations.
enum GoldenCheckRun {
    Clean,
    Failed {
        /// The console repair card `goldens check` prints.
        message: String,
        /// The raw violations behind the card.
        violations: Vec<String>,
    },
}

fn golden_check_run() -> Result<GoldenCheckRun, String> {
    let run_set = collect_golden_runs()?;
    // #8614: warn (do not fail) when a fixture input's working-tree endings
    // differ from its normalized form; `goldens bless` refuses on the same
    // evidence. Warning here keeps a local pass from looking clean.
    let crlf_warnings = crlf_guard_warnings(&run_set.fixtures);
    for warning in &crlf_warnings {
        eprintln!("warning: {warning}");
    }
    // One provenance snapshot backs both the drift report and the console
    // failure card, so the two always name the same build (#7401).
    let provenance = golden_drift_provenance();
    let entries = write_golden_drift_reports(&run_set.runs, &run_set.violations, &provenance)?;
    let mut body = goldens_check_report_body(&run_set.fixtures, &run_set.runs, &run_set.violations);
    if !crlf_warnings.is_empty() {
        body.push_str("## Line-ending warnings (#8614)\n\n");
        for warning in &crlf_warnings {
            body.push_str(&format!("- {warning}\n"));
        }
        body.push('\n');
    }
    write_report("goldens.md", &body)?;
    if run_set.violations.is_empty() {
        Ok(GoldenCheckRun::Clean)
    } else {
        let message = goldens_check_failure_message(&entries, &run_set.violations, &provenance);
        Ok(GoldenCheckRun::Failed {
            message,
            violations: run_set.violations,
        })
    }
}

/// Start of the violation `preparation::preparation_error` produces when a
/// fixture's locked dependencies cannot be prepared in this environment.
const FIXTURE_PREPARATION_FAILURE_PREFIX: &str = "fixture dependency preparation failed";

/// What `goldens check` established when run from `precommit`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PrecommitGoldens {
    /// Every fixture ran and matched its goldens.
    Verified,
    /// No fixture drifted, but some could not be compared because their locked
    /// dependencies are unavailable here. This is an environment gap, not
    /// drift and not a pass: hosted CI still owns those fixtures.
    EnvironmentGap(Vec<String>),
}

/// Run `goldens check` for `precommit`. Real drift or any other fixture error
/// fails; fixture-dependency preparation failures alone are reported as an
/// environment gap so a sandbox without the dependency cache is neither told
/// to bless availability drift nor shown a false pass.
pub(crate) fn goldens_check_for_precommit() -> Result<PrecommitGoldens, String> {
    match golden_check_run()? {
        GoldenCheckRun::Clean => Ok(PrecommitGoldens::Verified),
        GoldenCheckRun::Failed {
            message,
            violations,
        } => classify_precommit_goldens(&violations).ok_or(message),
    }
}

/// `Some(EnvironmentGap)` only when every violation is a preparation failure;
/// `None` (a real failure) when any violation is drift or another error.
fn classify_precommit_goldens(violations: &[String]) -> Option<PrecommitGoldens> {
    let is_gap = |violation: &String| violation.starts_with(FIXTURE_PREPARATION_FAILURE_PREFIX);
    if violations.is_empty() || !violations.iter().all(is_gap) {
        return None;
    }
    Some(PrecommitGoldens::EnvironmentGap(
        violations
            .iter()
            .map(|violation| violation.lines().next().unwrap_or(violation).to_string())
            .collect(),
    ))
}

fn is_fixture_expectation_path(path: &str) -> bool {
    path.starts_with("fixtures/") && path.contains("/expected/")
}

/// The first changed path that can make a golden stale: output-surface code
/// (the renderers and domain vocabulary every golden is produced from) or a
/// fixture expectation itself. Analysis-only, docs-only and test-only diffs
/// do not pay for the fixture run.
pub(crate) fn golden_staleness_trigger(changed: &[String]) -> Option<&str> {
    changed
        .iter()
        .map(String::as_str)
        .find(|path| crate::is_output_surface_path(path) || is_fixture_expectation_path(path))
}

/// Run the assistant-loop-health corpus contract used by `goldens check`.
///
/// This is intentionally the same validator used by `check-fixture-contracts`:
/// the golden gate must fail when an editor-agent-loop expectation drifts, even
/// though that corpus is manifest-only and is not executed by `run_fixture`.
pub(crate) fn golden_assistant_loop_health_contract_violations_at(
    base: &Path,
) -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    crate::validate_assistant_loop_health_fixture_corpus_at(base, &mut violations)?;
    Ok(violations)
}

/// Build an actionable `goldens check` failure: each drifted fixture gets its
/// semantic drift type (the discriminator from [`golden_drift_type`]) and a
/// blessing-state note, non-drift violations (contract/run errors) are listed
/// verbatim, and the message ends with the reproduce + next-action commands. The
/// gate emits its own repair card instead of "failed; see report".
pub(crate) fn goldens_check_failure_message(
    entries: &[GoldenDriftEntry],
    violations: &[String],
    provenance: &GoldenDriftProvenance,
) -> String {
    let mut message = String::from(
        "goldens check failed; semantic drift detail in target/ripr/reports/golden-drift.md\n",
    );
    // The actual side came from one specific build; name it so a CI log and
    // a local run can be told apart without guessing (#7401).
    message.push_str(&format!(
        "  produced by: {} (binary `{}`, revision `{}`)\n",
        provenance.ripr_version, provenance.binary, provenance.revision
    ));
    for entry in entries {
        let blessing = if entry.blessing_reason_present {
            "blessing CHANGELOG present"
        } else {
            "no blessing CHANGELOG — re-bless if the flip is intended"
        };
        message.push_str(&format!(
            "  drift: fixture `{}` [{}]: {} ({})\n",
            entry.fixture,
            entry.surface,
            golden_drift_type(&entry.semantics),
            blessing,
        ));
        // The console should carry the first differing line: the
        // report file is runner-local, so a CI log is often the only
        // place the hint is visible (#3636 corpus lane).
        let expected_text = read_text_lossy(Path::new(&entry.expected));
        let actual_text = read_text_lossy(Path::new(&entry.actual));
        if let (Ok(expected_text), Ok(actual_text)) = (expected_text, actual_text)
            && let Some(hint) = first_line_difference(
                &normalize_golden_text(&expected_text),
                &normalize_golden_text(&actual_text),
            )
        {
            message.push_str(&format!("  first difference: {hint}\n"));
        }
    }
    // Violations that are not per-fixture output drift (contract or run errors)
    // are not represented in `entries`; surface them verbatim so nothing is lost.
    for violation in violations
        .iter()
        .filter(|violation| !violation.contains("drift for fixture"))
    {
        message.push_str(&format!("  error: {violation}\n"));
    }
    message.push_str("reproduce: cargo xtask goldens check\n");
    message.push_str(
        "next: inspect target/ripr/reports/golden-drift.md; if a flip is intended, run `cargo xtask goldens bless <fixture> --reason <reason>`",
    );
    message
}

pub(crate) fn golden_drift_impl() -> Result<(), String> {
    let run_set = collect_golden_runs()?;
    let provenance = golden_drift_provenance();
    write_golden_drift_reports(&run_set.runs, &run_set.violations, &provenance)?;
    if run_set
        .violations
        .iter()
        .any(|violation| !violation.contains("drift for fixture"))
    {
        Err(format!(
            "golden drift report had fixture errors; see target/ripr/reports/golden-drift.md\n{}",
            run_set.violations.join("\n")
        ))
    } else {
        Ok(())
    }
}

fn collect_golden_runs() -> Result<GoldenRunSet, String> {
    let fixture_dirs = fixture_dirs()?;
    let mut violations = golden_assistant_loop_health_contract_violations_at(Path::new(
        "fixtures/boundary_gap/expected/assistant-loop-health",
    ))?;
    // Parallelize fixture execution via rayon (#2415). Each fixture is independent
    // (disjoint input/expected dirs), so parallel runs produce identical output.
    // The sequential results are collected in input order for deterministic reports.
    let results: Vec<FixtureResult> = fixture_dirs
        .par_iter()
        .map(|fixture| {
            let contract_violations = fixture_contract_violations(fixture)?;
            if !contract_violations.is_empty() {
                return Ok((contract_violations, None));
            }
            match run_fixture(fixture) {
                Ok(run) => Ok((run.comparison_violations(), Some(run))),
                Err(err) => Err(err),
            }
        })
        .collect();

    let mut runs = Vec::new();
    for result in results {
        match result {
            Ok((vios, maybe_run)) => {
                violations.extend(vios);
                if let Some(run) = maybe_run {
                    runs.push(run);
                }
            }
            Err(err) => violations.push(err),
        }
    }
    Ok(GoldenRunSet {
        fixtures: fixture_dirs,
        runs,
        violations,
    })
}

/// One `git ls-files --eol` row: index/worktree ending classes and the raw
/// attribute text after `attr/` (for example `text=auto eol=lf` or `-text`).
#[derive(Clone, Debug, Eq, PartialEq)]
struct GitEolInfo {
    index: String,
    worktree: String,
    attrs: String,
}

fn parse_git_ls_files_eol(output: &[u8]) -> BTreeMap<String, GitEolInfo> {
    let mut map = BTreeMap::new();
    for entry in output.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(entry);
        let Some((meta, path)) = text.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        let (Some(index), Some(worktree)) = (fields.next(), fields.next()) else {
            continue;
        };
        let attrs = meta
            .split_once("attr/")
            .map(|(_, rest)| rest.trim().to_string())
            .unwrap_or_default();
        map.insert(
            path.replace('\\', "/"),
            GitEolInfo {
                index: index.strip_prefix("i/").unwrap_or(index).to_string(),
                worktree: worktree.strip_prefix("w/").unwrap_or(worktree).to_string(),
                attrs,
            },
        );
    }
    map
}

/// Fire only for the #8614 shape: LF in the index, CRLF/mixed in the working
/// tree, and attributes that normalize to LF. `-text`/`binary` fixtures and
/// `eol=crlf` fixtures keep their bytes by design and never fire.
fn git_eol_info_fires(info: &GitEolInfo) -> bool {
    if info.index != "lf" || !matches!(info.worktree.as_str(), "crlf" | "mixed") {
        return false;
    }
    let tokens: Vec<&str> = info.attrs.split_whitespace().collect();
    if tokens
        .iter()
        .any(|t| *t == "-text" || *t == "binary" || *t == "text=unset")
    {
        return false;
    }
    if tokens.contains(&"eol=crlf") {
        return false;
    }
    // With `text`/`text=auto`/`eol=lf` the normalized form is LF. With no
    // text attribute at all git does not normalize, so CRLF is the content.
    tokens
        .iter()
        .any(|t| *t == "eol=lf" || *t == "text" || *t == "text=auto" || *t == "text=set")
}

fn bytes_contain_crlf(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|pair| pair == b"\r\n")
}

fn bytes_look_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|byte| *byte == 0)
}

/// Files whose bytes feed a fixture run: `diff.patch` plus `input/**`.
fn fixture_input_files(fixture: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let diff = fixture.join("diff.patch");
    if diff.is_file() {
        files.push(diff);
    }
    let mut stack = vec![fixture.join("input")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn fixture_embeds_input_identity(fixture: &Path) -> bool {
    read_text_lossy(&fixture.join("expected/check.json"))
        .is_ok_and(|text| text.contains("input_identity"))
}

/// Pure guard core: `eol_by_path` keys are `/`-normalized paths as printed by
/// `git ls-files`; `attr_excluded` holds normalized paths whose attributes
/// (`-text`, `binary`, `eol=crlf`) exempt them. Files absent from
/// `eol_by_path` fall back to a byte check so untracked inputs and synthetic
/// trees are covered on any host.
fn crlf_guard_violations_with(
    fixture: &Path,
    eol_by_path: &BTreeMap<String, GitEolInfo>,
    attr_excluded: &BTreeSet<String>,
) -> Vec<String> {
    if !fixture_embeds_input_identity(fixture) {
        return Vec::new();
    }
    let mut violations = Vec::new();
    for file in fixture_input_files(fixture) {
        let normalized = normalize_path(&file);
        if attr_excluded.contains(&normalized) {
            continue;
        }
        if let Some(info) = eol_by_path.get(&normalized) {
            if git_eol_info_fires(info) {
                violations.push(format!(
                    "{normalized}: working-tree line endings are `{}` but the git index is `{}` (attr/{}); blessing from these bytes would embed an `input_identity` an `eol=lf` checkout cannot reproduce",
                    info.worktree, info.index, info.attrs
                ));
            }
            continue;
        }
        if let Ok(bytes) = fs::read(&file)
            && bytes_contain_crlf(&bytes)
            && !bytes_look_binary(&bytes)
        {
            violations.push(format!(
                "{normalized}: working-tree bytes contain CRLF line endings and no LF-normalized git index entry covers this file; blessing from these bytes would embed an `input_identity` an `eol=lf` checkout cannot reproduce"
            ));
        }
    }
    violations
}

fn git_attr_excluded(path: &str) -> bool {
    let Ok(text) = run_output_optional("git", &["check-attr", "text", "eol", "binary", "--", path])
    else {
        return false;
    };
    text.lines().any(|line| {
        let value = line.rsplit(": ").next().unwrap_or("");
        line.contains(": text: unset")
            || line.contains(": binary: set")
            || (line.contains(": eol: crlf") && value == "crlf")
    })
}

/// Production guard for one fixture: ask git for the normalized form via
/// `git ls-files --eol`, falling back to bytes for untracked inputs.
fn crlf_guard_violations(fixture: &Path) -> Result<Vec<String>, String> {
    if !fixture_embeds_input_identity(fixture) {
        return Ok(Vec::new());
    }
    let fixture_arg = normalize_path(fixture);
    let eol_by_path = run_output_bytes("git", &["ls-files", "--eol", "-z", "--", &fixture_arg])
        .map(|bytes| parse_git_ls_files_eol(&bytes))
        .unwrap_or_default();
    let mut attr_excluded = BTreeSet::new();
    for file in fixture_input_files(fixture) {
        let normalized = normalize_path(&file);
        if !eol_by_path.contains_key(&normalized) && git_attr_excluded(&normalized) {
            attr_excluded.insert(normalized);
        }
    }
    Ok(crlf_guard_violations_with(
        fixture,
        &eol_by_path,
        &attr_excluded,
    ))
}

fn crlf_guard_warnings(fixtures: &[PathBuf]) -> Vec<String> {
    let mut warnings = Vec::new();
    for fixture in fixtures {
        match crlf_guard_violations(fixture) {
            Ok(violations) => warnings.extend(violations.into_iter().map(|violation| {
                format!(
                    "{violation} (`goldens bless` will refuse this fixture until the file is converted to LF)"
                )
            })),
            Err(_) => continue,
        }
    }
    warnings
}

fn goldens_bless(name: &str, reason: &str) -> Result<(), String> {
    // Argument validation precedes the fixture run, so a malformed invocation
    // costs neither a build nor a discarded cache (#3054 review).
    validate_bless_reason(reason)?;
    let fixture = fixture_dir_for_name(name)?;
    if !fixture.exists() {
        return Err(format!(
            "fixture does not exist: {}",
            normalize_path(&fixture)
        ));
    }
    // #8614: refuse before any fixture run or write when an input file's
    // working-tree endings differ from its index/attribute-normalized form.
    // Blessing from CRLF bytes would embed an `input_identity` that an
    // `eol=lf` checkout cannot reproduce. Never normalize the file here.
    let crlf_violations = crlf_guard_violations(&fixture)?;
    if !crlf_violations.is_empty() {
        let mut message = format!(
            "goldens bless refused for fixture `{name}`: {} fixture input file(s) have working-tree line endings that differ from their index/attribute-normalized (LF) form, and `expected/check.json` embeds `input_identity`. Blessing now would embed a hash no `eol=lf` checkout reproduces. No files were changed.\n",
            crlf_violations.len()
        );
        for violation in &crlf_violations {
            message.push_str(&format!("  - {violation}\n"));
        }
        message.push_str(
            "next: convert the listed file(s) to LF (for example `git checkout -- <path>` on a clean file, or your editor's LF setting), verify `git ls-files --eol -- <path>` reports `w/lf`, then re-run bless",
        );
        return Err(message);
    }
    let run = run_fixture_outputs(&fixture)?;

    // Integrity guards (#2410): validate the run output BEFORE copying it
    // over the expected files. A regression that produces empty/garbage
    // output must not be silently blessed as the new golden.
    validate_bless_output(&run, &fixture)?;

    let expected = fixture.join("expected");
    fs::create_dir_all(&expected)
        .map_err(|err| format!("failed to create {}: {err}", normalize_path(&expected)))?;
    fs::copy(&run.check_json, expected.join("check.json")).map_err(|err| {
        format!(
            "failed to update {} from {}: {err}",
            normalize_path(&expected.join("check.json")),
            normalize_path(&run.check_json)
        )
    })?;
    fs::copy(&run.human_txt, expected.join("human.txt")).map_err(|err| {
        format!(
            "failed to update {} from {}: {err}",
            normalize_path(&expected.join("human.txt")),
            normalize_path(&run.human_txt)
        )
    })?;
    let expected_human_full = expected.join("human-full.txt");
    let updated_human_full = expected_human_full.exists();
    if updated_human_full {
        fs::copy(&run.human_full_txt, &expected_human_full).map_err(|err| {
            format!(
                "failed to update {} from {}: {err}",
                normalize_path(&expected_human_full),
                normalize_path(&run.human_full_txt)
            )
        })?;
    }
    let changelog = expected.join("CHANGELOG.md");
    let mut text = if changelog.exists() {
        read_text_lossy(&changelog)?
    } else {
        "# Golden Output Changes\n".to_string()
    };
    let mut entry = format!(
        "\n{}\n\nReason:\n{reason}\n\nCommand:\n`cargo xtask goldens bless {name} --reason \"...\"`\n\nUpdated:\n- `expected/check.json`\n- `expected/human.txt`\n",
        next_pending_heading(&text, name)
    );
    if updated_human_full {
        entry.push_str("- `expected/human-full.txt`\n");
    }
    text.push_str(&entry);
    fs::write(&changelog, text)
        .map_err(|err| format!("failed to write {}: {err}", normalize_path(&changelog)))?;
    let mut actual_outputs = format!(
        "- `{}`\n- `{}`\n",
        normalize_path(&run.check_json),
        normalize_path(&run.human_txt)
    );
    let mut updated_outputs = format!(
        "- `{}`\n- `{}`\n",
        normalize_path(&expected.join("check.json")),
        normalize_path(&expected.join("human.txt"))
    );
    if updated_human_full {
        actual_outputs.push_str(&format!("- `{}`\n", normalize_path(&run.human_full_txt)));
        updated_outputs.push_str(&format!("- `{}`\n", normalize_path(&expected_human_full)));
    }
    updated_outputs.push_str(&format!("- `{}`\n", normalize_path(&changelog)));
    let body = format!(
        "# ripr goldens bless report\n\nStatus: pass\n\nFixture:\n- `{}`\n\nReason:\n```text\n{reason}\n```\n\nActual outputs:\n{actual_outputs}\nUpdated:\n{updated_outputs}",
        normalize_path(&fixture)
    );
    write_report("goldens-bless.md", &body)
}

/// Unique heading for the next appended pending changelog entry.
///
/// MD024 (no-duplicate-heading) rejects repeated heading text, so each entry
/// heading embeds the fixture name plus a sequence number computed from every
/// pending heading already in the log — including legacy plain `## Pending`
/// headings — so repeated blesses of the same fixture never collide.
pub(crate) fn next_pending_heading(existing_log: &str, fixture_name: &str) -> String {
    // Pick the first UNUSED number rather than count+1: a deleted or
    // hand-edited entry must never cause a duplicate heading. A bare legacy
    // `## Pending` heading implicitly reserved number 1.
    let mut used: Vec<usize> = existing_log
        .lines()
        .filter(|line| line.starts_with("## Pending"))
        .filter_map(|line| {
            let open = line.rfind('(')?;
            let close = line.rfind(')')?;
            line[open + 1..close].trim().parse().ok()
        })
        .collect();
    if existing_log
        .lines()
        .any(|line| line.trim_end() == "## Pending")
    {
        used.push(1);
    }
    let mut candidate = 1usize;
    while used.contains(&candidate) {
        candidate += 1;
    }
    format!("## Pending — {fixture_name} ({candidate})")
}

pub(crate) fn fixture_dirs() -> Result<Vec<PathBuf>, String> {
    let fixtures_dir = Path::new("fixtures");
    if !fixtures_dir.exists() {
        return Ok(Vec::new());
    }
    let mut fixtures = Vec::new();
    for entry in
        fs::read_dir(fixtures_dir).map_err(|err| format!("failed to read fixtures: {err}"))?
    {
        let entry = entry.map_err(|err| format!("failed to read fixtures: {err}"))?;
        let path = entry.path();
        if path.is_dir() && !is_manifest_only_fixture_dir(&path) {
            fixtures.push(path);
        }
    }
    fixtures.sort();
    Ok(fixtures)
}

pub(crate) fn is_manifest_only_fixture_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "active-goal-authority-audit"
                    | "actionable-gap-outcomes-corpus"
                    | "blind_journey_contract"
                    | "blind_journey_execute"
                    | "blind_journey_installed_rust"
                    | "blind_journey_installed_python"
                    | "blind_journey_installed_typescript"
                    | "bun-ub-cross-language-dogfood"
                    | "convergence"
                    | "cross-language-oracle-graph-corpus"
                    | "editor_gap_cockpit"
                    | "editor_first_run_usability"
                    | "editor_first_pr_bridge"
                    | "editor_adoption_assurance"
                    | "editor_actionable_gap_queue"
                    | "evidence-promotion-honesty-corpus"
                    | "executed-control-contract"
                    | "evidence-quality-benchmark"
                    | "first_successful_pr"
                    | "finding-alignment-dogfood"
                    | "gap-decision-ledger"
                    | "intervention-study"
                    | "issue_lifecycle_attempts"
                    | "issue_lifecycle_contract_plan"
                    | "issue_lifecycle_intake"
                    | "issue_lifecycle_real_rows"
                    | "github_unanalyzed_states"
                    | "orchestration_attempt_receipts"
                    | "perl_lsp_facts_exporter"
                    | "perl-real-repo-evals"
                    | "perl_packet_contract_migration"
                    | "python"
                    | "python-eval-sweep"
                    | "python-judged-pr-panel"
                    | "python-real-repo-evals"
                    | "real-repair-attempts"
                    | "rust-verdict-corpus"
                    | "typescript-verdict-corpus"
                    | "python-verdict-corpus"
                    | "perl-verdict-corpus"
                    | "release_control"
                    | "release_denominator"
                    | "release_scope"
                    | "source_promotion"
                    | "surface-projection-alignment"
                    | "swarm-plan-packet-corpus"
                    | "work_portfolio"
                    | "work_selection_identity"
                    | "work_result_identity"
                    | "work_claim_identity"
                    | "work_wave_identity"
                    | "work_context_identity"
                    | "work_synthesis_identity"
                    | "work_burndown_identity"
                    | "work_intake_identity"
                    | "work_plan_identity"
                    | "work_adapter_identity"
                    | "repository_orientation"
                    | "typescript-bun-ub-calibration"
                    | "typescript-preview-false-actionable-audit"
                    | "typescript-preview-repair-loop"
                    | "user-surface-projection-alignment"
            )
        })
}

fn fixture_dir_for_name(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(format!("invalid fixture name `{name}`"));
    }
    Ok(Path::new("fixtures").join(name))
}

pub(crate) fn fixture_contract_violations(path: &Path) -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    if is_manifest_only_fixture_dir(path) {
        return Ok(violations);
    }
    let normalized = normalize_path(path);
    let spec = path.join("SPEC.md");
    let diff = path.join("diff.patch");
    let expected_check = path.join("expected/check.json");

    if !spec.exists() {
        violations.push(format!("{normalized} is missing SPEC.md"));
        return Ok(violations);
    }
    if !diff.exists() {
        violations.push(format!("{normalized} is missing diff.patch"));
    }
    if !expected_check.exists() {
        violations.push(format!("{normalized} is missing expected/check.json"));
    }

    let text = read_text_lossy(&spec)?;
    if !text.lines().any(|line| line.starts_with(SPEC_LINE_PREFIX)) {
        violations.push(format!(
            "{} is missing `Spec: RIPR-SPEC-NNNN`",
            normalize_path(&spec)
        ));
    } else if !text.lines().any(spec_line_cites_real_id) {
        // #7399: the scaffold's literal `NNNN` placeholder used to satisfy the
        // prefix check here and then fail `check-traceability` later.
        violations.push(format!(
            "{} cites no real spec id (a `Spec: RIPR-SPEC-` line needs four digits, not a placeholder); run `cargo xtask specs next` to allocate one or cite an existing docs/specs id",
            normalize_path(&spec)
        ));
    }
    for heading in ["## Given", "## When", "## Then", "## Must Not"] {
        if !has_markdown_heading(&text, heading) {
            violations.push(format!("{} is missing `{heading}`", normalize_path(&spec)));
        }
    }
    Ok(violations)
}

const SPEC_LINE_PREFIX: &str = "Spec: RIPR-SPEC-";

/// True when a `Spec: RIPR-SPEC-` line is followed by exactly four digits, the
/// same id shape `check-traceability` requires.
fn spec_line_cites_real_id(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(SPEC_LINE_PREFIX) else {
        return false;
    };
    let digits: String = rest.chars().take(4).collect();
    digits.len() == 4
        && digits.chars().all(|c| c.is_ascii_digit())
        && !rest
            .chars()
            .nth(4)
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[derive(Debug)]
pub(crate) struct FixtureRun {
    name: String,
    actual_dir: PathBuf,
    check_json: PathBuf,
    human_txt: PathBuf,
    human_full_txt: PathBuf,
    comparisons: Vec<GoldenComparison>,
}

impl FixtureRun {
    #[cfg(test)]
    pub(crate) fn comparisons_all_match(&self) -> bool {
        self.comparisons.iter().all(|comparison| comparison.matches)
    }

    // Test-only detail accessor so nested-child assertions (fixture_cache)
    // can print WHICH golden drifted and the first differing line instead of
    // a bare bool; the parent's captured child output is the only artifact
    // available when a CI flake reproduces on a hosted runner.
    #[cfg(test)]
    pub(crate) fn comparison_violations_for_tests(&self) -> Vec<String> {
        self.comparison_violations()
    }

    fn comparison_violations(&self) -> Vec<String> {
        self.comparisons
            .iter()
            .filter(|comparison| !comparison.matches)
            .map(|comparison| {
                let difference_hint = comparison
                    .first_difference
                    .as_ref()
                    .map(|hint| format!("\n  diff:    {hint}"))
                    .unwrap_or_default();
                format!(
                    "{} drift for fixture `{}`\n  expected: {}\n  actual:   {}{}",
                    comparison.surface,
                    self.name,
                    normalize_path(&comparison.expected),
                    normalize_path(&comparison.actual),
                    difference_hint
                )
            })
            .collect()
    }
}

#[derive(Debug)]
struct GoldenComparison {
    surface: &'static str,
    expected: PathBuf,
    actual: PathBuf,
    matches: bool,
    first_difference: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GoldenDriftSemantics {
    pub(crate) added_finding_ids: Vec<String>,
    pub(crate) removed_finding_ids: Vec<String>,
    pub(crate) changed_exposure_classes: Vec<String>,
    pub(crate) changed_probe_families: Vec<String>,
    pub(crate) changed_oracle_kinds: Vec<String>,
    pub(crate) changed_oracle_strengths: Vec<String>,
    pub(crate) changed_stop_reasons: Vec<String>,
    pub(crate) changed_recommendations: Vec<String>,
    pub(crate) static_language_terms: Vec<String>,
    pub(crate) changed_line_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct GoldenDriftEntry {
    pub(crate) fixture: String,
    pub(crate) surface: String,
    pub(crate) expected: String,
    pub(crate) actual: String,
    pub(crate) blessing_reason_required: bool,
    pub(crate) blessing_reason_present: bool,
    pub(crate) semantics: GoldenDriftSemantics,
}

#[derive(Debug)]
struct GoldenRunSet {
    fixtures: Vec<PathBuf>,
    runs: Vec<FixtureRun>,
    violations: Vec<String>,
}

pub(crate) fn run_fixture(path: &Path) -> Result<FixtureRun, String> {
    let run = run_fixture_outputs(path)?;
    let expected = path.join("expected");
    let fixture_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let comparisons = fixture_golden_comparisons(
        &fixture_name,
        &expected,
        &run.check_json,
        &run.human_txt,
        &run.human_full_txt,
    )?;
    Ok(FixtureRun { comparisons, ..run })
}

/// Validate the run output before blessing it as the new golden (#2410).
/// Refuses to copy files that are malformed, empty, or show a suspicious
/// finding-count collapse — the classic regression signature.
fn validate_bless_output(run: &FixtureRun, fixture: &Path) -> Result<(), String> {
    // 1. check.json must be valid JSON with expected top-level fields.
    let json_text = read_text_lossy(&run.check_json)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_text).map_err(|err| {
        format!(
            "goldens bless: check.json is not valid JSON for {}: {err}",
            run.name
        )
    })?;
    if parsed.get("findings").is_none() {
        return Err(format!(
            "goldens bless: check.json for {} has no 'findings' field — output may be malformed",
            run.name
        ));
    }

    // 2. Zero-finding guard: if the new output has 0 findings but the
    //    previous expected had >0, this is likely a regression. Compare
    //    against the existing expected/check.json before overwriting.
    let new_count = parsed["findings"].as_array().map(|a| a.len()).unwrap_or(0);
    if new_count == 0 {
        let expected_path = fixture.join("expected/check.json");
        if expected_path.exists()
            && let Ok(old_text) = read_text_lossy(&expected_path)
            && let Ok(old_json) = serde_json::from_str::<serde_json::Value>(&old_text)
        {
            let old_count = old_json["findings"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0);
            if old_count > 0 {
                return Err(format!(
                    "goldens bless: check.json for {} dropped from {old_count} to 0 findings — \
                     this likely indicates a regression, not a golden update. \
                     If intentional, inspect the diff manually before re-blessing.",
                    run.name
                ));
            }
        }
    }

    // 3. human.txt must be non-empty.
    let human_text = read_text_lossy(&run.human_txt)?;
    if human_text.trim().is_empty() {
        return Err(format!(
            "goldens bless: human.txt for {} is empty — output may be malformed",
            run.name
        ));
    }

    Ok(())
}

/// Environment variable `ripr` reads to relocate its fact cache base
/// (`crates/ripr/src/analysis/seam_cache.rs`, `CACHE_DIR_ENV`).
const FIXTURE_CACHE_DIR_ENV: &str = "RIPR_CACHE_DIR";

/// Where a fixture run's fact cache lives.
///
/// #3054: `ripr` derives its default cache base from the workspace root it is
/// analyzing, and a fixture run is rooted at `<fixture>/input`. Left to the
/// default, each fixture writes into its own `input/target/ripr/cache` inside
/// the corpus, so the repository-level `target/ripr/cache` that CI clears is not
/// the cache a fixture run reads. Rather than infer 222 scattered locations and
/// delete inside the tracked corpus, the runner dictates one: an absolute,
/// per-fixture directory under the repository build tree. The path is then a
/// fact the runner owns, not a guess about the child.
///
/// Per fixture rather than per corpus so the three output surfaces of one
/// fixture still share a warm cache within a single run.
pub(crate) fn fixture_cache_dir(name: &str) -> Result<PathBuf, String> {
    let dir = Path::new("target")
        .join("ripr")
        .join("fixture-cache")
        .join(name);
    std::path::absolute(&dir)
        .map_err(|err| format!("resolve fixture cache dir {}: {err}", normalize_path(&dir)))
}

/// Discard a fixture's cached facts so the run recomputes them under the binary
/// this process just built.
///
/// A cache that could not be removed is reported, not swallowed: a gate that
/// cannot distinguish "cleared" from "a lock or permission denied the clear" is
/// the false-confidence surface #3054 is about.
fn clear_fixture_cache(dir: &Path) -> Result<(), String> {
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "failed to clear fixture fact cache {}: {err}",
            normalize_path(dir)
        )),
    }
}

pub(crate) fn run_fixture_outputs(path: &Path) -> Result<FixtureRun, String> {
    let name = fixture_name(path)?;
    if name == observation_tail_return::FIXTURE {
        observation_tail_return::validate(path)?;
    }
    let diff = path.join("diff.patch");
    let input = path.join("input");
    if !diff.exists() {
        return Err(format!("{} is missing diff.patch", normalize_path(path)));
    }
    if !input.exists() {
        return Err(format!(
            "{} is missing input/ fixture workspace",
            normalize_path(path)
        ));
    }
    if name == preparation::FIXTURE {
        preparation::ensure_ready()?;
    }

    let actual_dir = Path::new("target")
        .join("ripr")
        .join("fixtures")
        .join(&name);
    fs::create_dir_all(&actual_dir).map_err(|err| {
        format!(
            "failed to create actual fixture output directory {}: {err}",
            normalize_path(&actual_dir)
        )
    })?;

    let check_json = actual_dir.join("check.json");
    let human_txt = actual_dir.join("human.txt");
    let human_full_txt = actual_dir.join("human-full.txt");
    let root = normalize_path(&input);
    let diff_file = normalize_path(&diff);

    // #3054: clear before the first surface, then let all three share the warm
    // entry. The clear must happen here, next to the runs it governs, so no
    // command entry point can reach the fixture corpus without it.
    let cache_dir = fixture_cache_dir(&name)?;
    clear_fixture_cache(&cache_dir)?;

    let json = normalize_fixture_json_output(&run_fixture_check(
        &root,
        &diff_file,
        FixtureCheckFormat::Json,
        Some(&cache_dir),
    )?);
    fs::write(&check_json, json).map_err(|err| {
        format!(
            "failed to write actual fixture output {}: {err}",
            normalize_path(&check_json)
        )
    })?;

    let human = normalize_fixture_human_output(&run_fixture_check(
        &root,
        &diff_file,
        FixtureCheckFormat::Human,
        Some(&cache_dir),
    )?);
    fs::write(&human_txt, human).map_err(|err| {
        format!(
            "failed to write actual fixture output {}: {err}",
            normalize_path(&human_txt)
        )
    })?;

    let human_full = normalize_fixture_human_output(&run_fixture_check(
        &root,
        &diff_file,
        FixtureCheckFormat::HumanFull,
        Some(&cache_dir),
    )?);
    fs::write(&human_full_txt, human_full).map_err(|err| {
        format!(
            "failed to write actual fixture output {}: {err}",
            normalize_path(&human_full_txt)
        )
    })?;

    Ok(FixtureRun {
        name,
        actual_dir,
        check_json,
        human_txt,
        human_full_txt,
        comparisons: Vec::new(),
    })
}

#[derive(Clone, Copy)]
pub(crate) enum FixtureCheckFormat {
    Json,
    Human,
    HumanFull,
}

/// Memoized result of the one `cargo build -p ripr` this process owes the
/// fixture corpus. See [`ripr_fixture_binary`].
static RIPR_BINARY_BUILD: OnceLock<Result<(), String>> = OnceLock::new();

/// The worktree-absolute debug binary, built once per xtask process (#2110,
/// #3054): each scenario previously paid a `cargo run` spawn plus link check;
/// the absolute path keeps a long `../` from resolving to a stale binary in the
/// enclosing checkout.
///
/// #3054: this used to build only `if !binary.exists()`, so a `target/debug/ripr`
/// left by an earlier build was reused verbatim and `goldens check` could report
/// zero drift while validating previous code. The build now belongs to this
/// function — the single place every fixture, golden, drift, and dogfood spawn
/// reaches the binary through — rather than to individual command entry points,
/// where a newly added entry point (as `golden-drift` was) silently opts out.
pub(crate) fn ripr_fixture_binary() -> Result<String, String> {
    if isolate_fixture_binary() {
        let target = isolated_default_target_dir(&ripr_debug_binary())?;
        let binary = ripr_debug_binary_in(Some(target.clone()));
        return ripr_fixture_binary_built_by(binary, &RIPR_ISOLATED_BINARY_BUILD, &|| {
            preparation::build_default_isolated(&target)
        });
    }
    // ripr_debug_binary() honors CARGO_TARGET_DIR (#2176 review): the
    // routed CI jobs set it, so `cargo build` writes there.
    ripr_fixture_binary_built_by(ripr_debug_binary(), &RIPR_BINARY_BUILD, &|| {
        // The build output goes straight to the console here, so the retry
        // cannot read why it failed; it is Windows-only and bounded.
        crate::run::retry_while_binary_busy(
            |_| true,
            || preparation::build(&["build", "-p", "ripr"]),
        )
    })
}

/// Set by a lane whose test run is itself built with a different feature set
/// than the fixture corpus (Test Analytics: `nextest --all-features`). There the
/// suite's `target/debug/ripr` is the all-features build, and the corpus's plain
/// `cargo build -p ripr` would relink it to the default-feature build mid-suite,
/// so feature-aware tests would race the swap (#11602). With the variable set,
/// the corpus builds its default binary in a private target directory instead.
/// Unset (the required lane builds default features throughout), behavior is
/// unchanged and the shared binary is rebuilt in place as before (#3054).
pub(crate) const FIXTURE_BINARY_ISOLATE_ENV: &str = "RIPR_FIXTURE_BINARY_ISOLATE";

fn isolate_fixture_binary() -> bool {
    std::env::var_os(FIXTURE_BINARY_ISOLATE_ENV).is_some_and(|value| value == "1")
}

/// Private target for the isolated default-feature build, beside (not inside)
/// the Perl variant's own `debug/` so the two feature sets never thrash one
/// Cargo fingerprint.
pub(crate) fn isolated_default_target_dir(shared: &Path) -> Result<PathBuf, String> {
    let root = shared
        .parent()
        .and_then(Path::parent)
        .ok_or("missing shared target root")?;
    std::path::absolute(root.join("fixture-variants").join("default"))
        .map_err(|err| format!("resolve isolated default target: {err}"))
}

static RIPR_ISOLATED_BINARY_BUILD: OnceLock<Result<(), String>> = OnceLock::new();

/// Memoized result of the one `cargo build -p ripr --features lang-perl` this
/// process owes the Perl verdict corpus. See [`ripr_perl_fixture_binary`].
static RIPR_PERL_BINARY_BUILD: OnceLock<Result<(), String>> = OnceLock::new();

#[cfg(test)]
pub(crate) mod variant_test {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    type RecordedCommand = (String, Vec<String>, Option<PathBuf>);

    pub(crate) struct Context {
        pub(crate) shared: PathBuf,
        pub(crate) once: OnceLock<Result<(), String>>,
        pub(crate) files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
        pub(crate) commands: RefCell<Vec<RecordedCommand>>,
        pub(crate) writes: RefCell<Vec<PathBuf>>,
    }

    impl Context {
        pub(crate) fn write(&self, path: PathBuf, bytes: Vec<u8>) {
            self.writes.borrow_mut().push(path.clone());
            self.files.borrow_mut().insert(path, bytes);
        }

        pub(crate) fn shared_generation(&self) -> usize {
            self.writes
                .borrow()
                .iter()
                .filter(|p| **p == self.shared)
                .count()
        }
    }

    thread_local! { static ACTIVE: RefCell<Option<Rc<Context>>> = const { RefCell::new(None) }; }

    pub(crate) fn active() -> Option<Rc<Context>> {
        ACTIVE.with(|slot| slot.borrow().clone())
    }

    pub(crate) fn with_context<T>(context: Rc<Context>, f: impl FnOnce() -> T) -> T {
        struct Restore(Option<Rc<Context>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE.with(|slot| *slot.borrow_mut() = self.0.take());
            }
        }
        let old = ACTIVE.with(|slot| slot.borrow_mut().replace(context));
        let _restore = Restore(old);
        f()
    }

    pub(super) fn run(
        program: &str,
        args: &[&str],
        envs: &[(&str, &str)],
    ) -> Option<Result<(), String>> {
        let context = active()?;
        let target = envs
            .iter()
            .rev()
            .find(|(key, _)| *key == "CARGO_TARGET_DIR")
            .map(|(_, value)| PathBuf::from(value));
        context.commands.borrow_mut().push((
            program.into(),
            args.iter().map(|s| (*s).into()).collect(),
            target.clone(),
        ));
        let output = target
            .map(|dir| crate::ripr_debug_binary_in(Some(dir)))
            .unwrap_or_else(|| context.shared.clone());
        context.write(output, b"perl".to_vec());
        Some(Ok(()))
    }

    pub(super) fn copy(source: &Path, destination: &Path) -> Option<Result<(), String>> {
        let context = active()?;
        let Some(bytes) = context.files.borrow().get(source).cloned() else {
            return Some(Err(format!("missing source {}", source.display())));
        };
        context.write(destination.to_path_buf(), bytes);
        Some(Ok(()))
    }
}

/// The debug binary with the `lang-perl` preview adapter compiled in, for the
/// Perl verdict corpus (RIPR-SPEC-0245): the default build refuses Perl
/// packets. The variant builds in a dedicated target directory
/// (#11197): it used to relink the shared
/// `target/debug/ripr` to the Perl feature set mid-suite and restore it
/// afterwards, so a cross-process consumer of the suite binary could observe
/// a different analyzer identity between two execs. The built variant is
/// still copied beside the shared binary under its own `ripr-lang-perl`
/// name; the shared `target/debug/ripr` is never rewritten. No build lock is
/// taken: the fixture-cache tests hold Cargo's own lock and need the child
/// to wait there. Once the variant target is warm, only the `ripr` crate
/// recompiles.
pub(crate) fn ripr_perl_fixture_binary() -> Result<String, String> {
    #[cfg(test)]
    let context = variant_test::active();
    #[cfg(test)]
    let (shared, once) = context.as_ref().map_or_else(
        || (ripr_debug_binary(), &RIPR_PERL_BINARY_BUILD),
        |context| (context.shared.clone(), &context.once),
    );
    #[cfg(not(test))]
    let (shared, once) = (ripr_debug_binary(), &RIPR_PERL_BINARY_BUILD);
    fixture_variant_binary_built_by(
        shared,
        once,
        &preparation::build_perl_variant,
        &|source, destination| {
            #[cfg(test)]
            if let Some(result) = variant_test::copy(source, destination) {
                return result;
            }
            fs::copy(source, destination).map(|_| ()).map_err(|err| {
                format!(
                    "copy {} to {} failed: {err}",
                    source.display(),
                    destination.display()
                )
            })
        },
    )
}

pub(crate) fn fixture_variant_binary_built_by(
    shared: PathBuf,
    build_once: &OnceLock<Result<(), String>>,
    build: &(dyn Fn(&Path) -> Result<(), String> + Sync),
    copy: &(dyn Fn(&Path, &Path) -> Result<(), String> + Sync),
) -> Result<String, String> {
    let target = shared
        .parent()
        .and_then(Path::parent)
        .ok_or("missing shared target root")?;
    let variant_target = std::path::absolute(target.join("fixture-variants"))
        .map_err(|err| format!("resolve variant target: {err}"))?;
    let perl = shared.with_file_name(format!("ripr-lang-perl{}", std::env::consts::EXE_SUFFIX));
    let variant = ripr_debug_binary_in(Some(variant_target.clone()));
    ripr_fixture_binary_built_by(perl.clone(), build_once, &|| {
        build(&variant_target)?;
        copy(&variant, &perl)
    })
}

/// Resolve `binary` after ensuring `build` has run exactly once for `build_once`.
///
/// The build is unconditional with respect to the binary already existing — that
/// is the whole point of #3054 — and memoized with respect to the process, so the
/// rayon-parallel fixture runs do not queue one `cargo build` per fixture behind
/// the package lock. A failed build is remembered and re-reported; it never
/// degrades into "resolved a path that may be stale".
fn ripr_fixture_binary_built_by(
    binary: PathBuf,
    build_once: &OnceLock<Result<(), String>>,
    build: &(dyn Fn() -> Result<(), String> + Sync),
) -> Result<String, String> {
    build_once.get_or_init(build).clone()?;
    std::path::absolute(&binary)
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|err| format!("resolve {} failed: {err}", binary.display()))
}

/// Run `ripr check` for one fixture surface.
///
/// `cache_dir` pins the child's fact cache. Fixture-corpus runs pass `Some`,
/// because `ripr` resolves its cache from the `--root` it is given: without a
/// pin, a fixture's cache lands in `<fixture>/input/target/ripr/cache` and no
/// amount of clearing at the repository root touches it (#3054). Dogfood scenarios
/// pass `None` — they analyze real repositories whose workspace cache is
/// content-keyed on sources that change with the edit under test, so relocating
/// it would buy nothing and cost every dogfood run a cold analysis.
///
/// Corpus children use one Rayon worker because the parent already runs cases
/// in parallel. This bounds nested worker pools; helper/I/O threads remain.
pub(crate) fn run_fixture_check(
    root: &str,
    diff_file: &str,
    format: FixtureCheckFormat,
    cache_dir: Option<&Path>,
) -> Result<String, String> {
    let binary = ripr_fixture_binary()?;
    let mut args = vec![
        "check".to_string(),
        "--root".to_string(),
        root.to_string(),
        "--diff".to_string(),
        diff_file.to_string(),
        "--mode".to_string(),
        "fast".to_string(),
    ];
    match format {
        FixtureCheckFormat::Json => args.push("--json".to_string()),
        // Goldens are one deterministic projection: pin the Evidence legend
        // that the CLI collapses after the first run in a build tree.
        FixtureCheckFormat::Human => args.push("--legend".to_string()),
        FixtureCheckFormat::HumanFull => {
            args.push("--format".to_string());
            args.push("human-full".to_string());
        }
    }
    let environment = preparation::environment()?;
    let mut envs = environment
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    match cache_dir {
        Some(dir) => {
            let value = dir.to_string_lossy().into_owned();
            envs.push((FIXTURE_CACHE_DIR_ENV, &value));
            envs.push(("RAYON_NUM_THREADS", "1"));
            run_output_owned_with_envs(&binary, &args, &envs)
        }
        None => run_output_owned_with_envs(&binary, &args, &envs),
    }
}

fn fixture_golden_comparisons(
    fixture: &str,
    expected: &Path,
    check_json: &Path,
    human_txt: &Path,
    human_full_txt: &Path,
) -> Result<Vec<GoldenComparison>, String> {
    let mut comparisons = Vec::new();
    comparisons.push(compare_golden(
        fixture,
        "check.json",
        &expected.join("check.json"),
        check_json,
    )?);

    let expected_human = expected.join("human.txt");
    if expected_human.exists() {
        comparisons.push(compare_golden(
            fixture,
            "human.txt",
            &expected_human,
            human_txt,
        )?);
    }
    let expected_human_full = expected.join("human-full.txt");
    if expected_human_full.exists() {
        comparisons.push(compare_golden(
            fixture,
            "human-full.txt",
            &expected_human_full,
            human_full_txt,
        )?);
    }
    Ok(comparisons)
}

fn compare_golden(
    fixture: &str,
    surface: &'static str,
    expected: &Path,
    actual: &Path,
) -> Result<GoldenComparison, String> {
    let expected_text = read_text_lossy(expected)?;
    let expected_text = if surface != "check.json" {
        project_windows_python_verify_expected(fixture, &expected_text, cfg!(windows))
    } else {
        expected_text
    };
    let actual_text = read_text_lossy(actual)?;
    let (expected_text, actual_text) = if surface == "check.json" {
        (
            mask_check_json_test_locations(&normalize_check_json_golden_document(&expected_text)),
            mask_check_json_test_locations(&normalize_check_json_golden_document(&actual_text)),
        )
    } else {
        (
            mask_test_file_citations(&mask_repair_refusal_reason(&expected_text)),
            mask_test_file_citations(&mask_repair_refusal_reason(&actual_text)),
        )
    };
    let normalized_expected = normalize_golden_text(&expected_text);
    let normalized_actual = normalize_golden_text(&actual_text);
    let aliased = surface != "check.json"
        && transitional_refusal_aliases_apply(
            fixture,
            surface,
            &normalized_expected,
            &normalized_actual,
        );
    Ok(GoldenComparison {
        surface,
        expected: expected.to_path_buf(),
        actual: actual.to_path_buf(),
        matches: aliased || normalized_expected == normalized_actual,
        first_difference: first_line_difference(&normalized_expected, &normalized_actual),
    })
}

/// Fixture goldens pin the Bash action. On Windows the product instead gives
/// manual guidance; project only exact, simple fixture-owned actions, keeping
/// the selected test token and requiring the expected fixture root.
fn project_windows_python_verify_expected(fixture: &str, text: &str, windows: bool) -> String {
    if !windows {
        return text.to_string();
    }
    let root = format!("<cwd>/fixtures/{fixture}/input");
    text.split_inclusive('\n')
        .map(|line| {
            let (body, newline) = line.strip_suffix('\n').map_or((line, ""), |s| (s, "\n"));
            let (prefix, replacement) = if let Some(rest) =
                body.strip_prefix("  Verify command (Bash): (cd -P -- ")
            {
                (
                    rest,
                    "  Suggested command (unverified; inspect and run from the selected root): ",
                )
            } else if let Some(rest) = body.strip_prefix("  verify (Bash): (cd -P -- ") {
                (
                    rest,
                    "  suggested command (unverified; inspect and run from the selected root): ",
                )
            } else {
                return line.to_string();
            };
            let Some((declared_root, selector)) = prefix.split_once(" && python -m pytest ") else {
                return line.to_string();
            };
            let Some(selector) = selector.strip_suffix(") (high)") else {
                return line.to_string();
            };
            if declared_root != root
                || selector.is_empty()
                || !selector
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '-'))
            {
                return line.to_string();
            }
            format!("{replacement}python -m pytest {selector}{newline}")
        })
        .collect()
}

pub(crate) fn normalize_golden_text(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n");
    normalized
        .strip_suffix('\n')
        .unwrap_or(&normalized)
        .to_string()
}

pub(crate) fn first_line_difference(expected: &str, actual: &str) -> Option<String> {
    let expected_lines: Vec<&str> = expected.split('\n').collect();
    let actual_lines: Vec<&str> = actual.split('\n').collect();
    let max_len = expected_lines.len().max(actual_lines.len());

    for index in 0..max_len {
        let expected_line = expected_lines.get(index).copied().unwrap_or("<missing>");
        let actual_line = actual_lines.get(index).copied().unwrap_or("<missing>");
        if expected_line != actual_line {
            let mut detail = format!(
                "line {} expected `{}` vs actual `{}`",
                index + 1,
                snapshot_line_preview(expected_line),
                snapshot_line_preview(actual_line)
            );
            // Long lines whose previews are identical need a divergence
            // pointer, or the hint names no actionable difference.
            if snapshot_line_preview(expected_line) == snapshot_line_preview(actual_line) {
                let common = expected_line
                    .chars()
                    .zip(actual_line.chars())
                    .take_while(|(expected, actual)| expected == actual)
                    .count();
                let start = common.saturating_sub(40);
                let expected_excerpt: String = expected_line.chars().skip(start).take(80).collect();
                let actual_excerpt: String = actual_line.chars().skip(start).take(80).collect();
                detail.push_str(&format!(
                    "; divergence at char {common}: expected `...{expected_excerpt}...` vs actual `...{actual_excerpt}...`"
                ));
            }
            return Some(detail);
        }
    }

    None
}

fn snapshot_line_preview(line: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 120;

    if line == "<missing>" {
        return line.to_string();
    }

    let escaped = line.escape_debug().to_string().replace('`', "\\`");
    let mut preview: String = escaped.chars().take(MAX_PREVIEW_CHARS).collect();
    if escaped.chars().count() > MAX_PREVIEW_CHARS {
        preview.push('…');
    }
    preview
}

/// Classification goldens pin analyzer findings, not the check-JSON
/// canonical next-action projection (#7258). Strip that one last member so
/// embedding it on the public CLI path does not force a corpus-wide golden
/// refresh. Dedicated CLI/schema tests own the field.
fn strip_canonical_next_action_member(value: &str) -> String {
    const MARKER: &str = "\n  \"canonical_next_action\": ";
    let Some(start) = value.rfind(MARKER) else {
        return value.to_string();
    };
    let rest = &value[start + MARKER.len()..];
    // The check JSON adapter appends a compact one-line value, then the
    // object's closing brace. The first newline after the marker is that
    // terminator (`\n}` plus an optional trailing newline), not a later
    // trailing newline that would drop the brace.
    let Some(end) = rest.find('\n') else {
        return value.to_string();
    };
    let mut stripped = value[..start].to_string();
    if stripped.ends_with(',') {
        stripped.pop();
    }
    stripped.push_str(&rest[end..]);
    stripped
}

/// Last-member append reconstitutes a pretty root closer (`]\n}`). One
/// classification golden still stores compact `]}`. Normalize both so
/// embedding `canonical_next_action` does not look like findings drift.
fn split_compact_root_object_closer(value: &str) -> String {
    let trailing_newline = value.ends_with('\n');
    let trimmed = value.trim_end();
    let Some(without_brace) = trimmed.strip_suffix('}') else {
        return value.to_string();
    };
    if without_brace.ends_with('\n') {
        return value.to_string();
    }
    let Some(without_bracket) = without_brace.strip_suffix(']') else {
        return value.to_string();
    };
    // Keep one-line documents such as `{"findings":[]}` intact. Compact
    // fixture goldens place `]}` after indent: `  ]}`.
    if !without_bracket.ends_with(|ch: char| ch.is_whitespace()) {
        return value.to_string();
    }
    let mut split = String::with_capacity(value.len() + 1);
    split.push_str(without_bracket);
    split.push(']');
    split.push('\n');
    split.push('}');
    if trailing_newline {
        split.push('\n');
    }
    split
}

/// Marker that opens the human screen's repair-start refusal sentence.
const REPAIR_REFUSAL_MARKER: &str = "no repair start offered: ";

/// One `RepairRouteRefusal` variant whose sentence embeds a value that moves
/// with unrelated code. Everything after `prefix` is replaced by `<reason>`.
struct RefusalRule {
    /// Fixed opening of the variant's sentence (the discriminator), compared
    /// verbatim.
    prefix: &'static str,
}

const fn rule(prefix: &'static str) -> RefusalRule {
    RefusalRule { prefix }
}

/// Mirrors the arms of `RepairRouteRefusal::advisory`
/// (crates/ripr/src/app/navigation.rs) whose sentence carries a variable part
/// (a language, file, command or evidence list). A reason that matches no rule
/// is compared verbatim, so every arm with fixed wording (no gap ID, no
/// binding seam, several seams, already exposed, gap/limit conflict, config
/// unreadable, incomplete static evidence, seam budget) is deliberately absent
/// here: rewording one is visible golden drift that has to be re-blessed
/// (#12003). Add an arm only when its sentence embeds a value that moves with
/// unrelated code, and mask just that value.
const REPAIR_REFUSAL_RULES: &[RefusalRule] = &[
    rule("the seam inventory behind `ripr repair`"),
    rule("classifying the finding's seams failed"),
];

/// Mask the moving values of the repair-start refusal sentence in human
/// goldens. Arms with fixed wording are compared verbatim (#12003).
///
/// Masking the whole tail of every refusal let wording regress with the gate
/// green (#12003), so only an arm whose sentence embeds a language, file or
/// command (`REPAIR_REFUSAL_RULES`) and the seam count keep a `<reason>` mask;
/// all other arms, including their evidence lists and advice, are compared
/// verbatim. A repair start that flips to a refusal, or the refusal line
/// disappearing, is also drift.
fn mask_repair_refusal_reason(value: &str) -> String {
    let mut masked = String::with_capacity(value.len());
    for segment in value.split_inclusive('\n') {
        let indent = segment.len() - segment.trim_start().len();
        let (head, rest) = segment.split_at(indent);
        let Some(reason) = rest.strip_prefix(REPAIR_REFUSAL_MARKER) else {
            masked.push_str(segment);
            continue;
        };
        let body_end = reason.trim_end_matches(['\r', '\n']).len();
        let (body, ending) = reason.split_at(body_end);
        masked.push_str(head);
        masked.push_str(REPAIR_REFUSAL_MARKER);
        masked.push_str(&mask_refusal_body(body));
        masked.push_str(ending);
    }
    masked
}

const SEAMS_UNTIED_OPENING: &str = transitional::SEAMS_UNTIED;

/// Whether `actual` differs from `expected` (both already refusal-masked) only
/// by refusal openings approved for this exact `fixture/surface` in
/// `transitional::MIGRATIONS`. A migration to the exposed opening also needs
/// the expected golden to say `Static exposure: exposed` itself.
fn transitional_refusal_aliases_apply(
    fixture: &str,
    surface: &str,
    expected: &str,
    actual: &str,
) -> bool {
    let key = format!("{fixture}/{surface}");
    let approved: Vec<&transitional::Migration> = transitional::MIGRATIONS
        .iter()
        .filter(|migration| migration.surfaces.contains(&key.as_str()))
        .collect();
    if approved.is_empty() {
        return false;
    }
    let expected_lines: Vec<&str> = expected.split('\n').collect();
    let actual_lines: Vec<&str> = actual.split('\n').collect();
    if expected_lines.len() != actual_lines.len() {
        return false;
    }
    let exposed_in_expected = expected_lines
        .iter()
        .any(|line| line.trim_start().starts_with("Static exposure: exposed"));
    expected_lines.iter().zip(&actual_lines).all(|(old, new)| {
        if old == new {
            return true;
        }
        // The opening may move, the line's indentation may not.
        let indent = |line: &str| line[..line.len() - line.trim_start().len()].to_string();
        if indent(old) != indent(new) {
            return false;
        }
        let (Some(old_body), Some(new_body)) = (
            old.trim_start().strip_prefix(REPAIR_REFUSAL_MARKER),
            new.trim_start().strip_prefix(REPAIR_REFUSAL_MARKER),
        ) else {
            return false;
        };
        approved.iter().any(|migration| {
            old_body.starts_with(migration.old)
                && new_body.starts_with(migration.new)
                && (migration.new != transitional::EXPOSED || exposed_in_expected)
        })
    })
}

/// Replace the seam count of the untied-seams opening so the count (which
/// moves with unrelated seams) is not golden-pinned.
fn normalize_seam_count(body: &str) -> Option<String> {
    let rest = body.strip_prefix("ripr found ")?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let after = &rest[digits..];
    let after = after
        .strip_prefix(" spots")
        .or_else(|| after.strip_prefix(" spot"))
        .or_else(|| after.strip_prefix(" seams"))
        .or_else(|| after.strip_prefix(" seam"))?;
    after.strip_prefix(
        " in this finding's file, but could not tie one to this finding's changed line",
    )?;
    Some(format!("{SEAMS_UNTIED_OPENING}<reason>"))
}

/// Opening of the `PreviewPacketNotComplete` sentence (#12086). It embeds the
/// language and a reason, so it is not a fixed-wording arm; only the test
/// location inside the reason moves with unrelated edits.
const PREVIEW_PACKET_REFUSAL_OPENING: &str = "ripr has no complete repair packet for this ";

/// Where a TypeScript blocker reason cites a test: `the test at `path:line``.
const PREVIEW_TEST_LOCATION_OPENING: &str = "the test at `";

/// Mask the backticked `path:line` a `PreviewPacketNotComplete` reason cites
/// (#12086). The language, the blocker wording and the rest of the sentence
/// stay compared, so rewording any of them is drift while adding a line above
/// the cited test is not. A reason with no cited location (for example the
/// moved-assertion-selection blocker, which names a test, not a location) is
/// compared verbatim.
fn normalize_preview_packet_location(body: &str) -> Option<String> {
    if !body.starts_with(PREVIEW_PACKET_REFUSAL_OPENING) {
        return None;
    }
    let start = body.find(PREVIEW_TEST_LOCATION_OPENING)? + PREVIEW_TEST_LOCATION_OPENING.len();
    let len = body[start..].find('`')?;
    // An empty span cites nothing; leave it verbatim so it reads as drift.
    if len == 0 {
        return None;
    }
    Some(format!(
        "{}<location>{}",
        &body[..start],
        &body[start + len..]
    ))
}

/// Filename suffixes of JS/TS test files whose cited line moves when an
/// unrelated edit shifts the test (#12163).
const TEST_FILE_SUFFIXES: &[&str] = &[
    ".test.ts",
    ".test.tsx",
    ".test.js",
    ".test.jsx",
    ".test.mjs",
    ".test.cjs",
    ".spec.ts",
    ".spec.tsx",
    ".spec.js",
    ".spec.jsx",
    ".spec.mjs",
    ".spec.cjs",
];

fn ends_with_test_file(text: &str) -> bool {
    TEST_FILE_SUFFIXES
        .iter()
        .any(|suffix| text.ends_with(suffix))
}

/// Replace the line in every `<test file>:<line>` citation with `<line>`
/// (#12163). Only a JS/TS test-file path immediately followed by `:` and
/// digits is touched, so source locations, blocker wording and every other
/// number stay compared.
fn mask_test_file_citations(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(colon) = rest.find(':') {
        let (head, tail) = rest.split_at(colon);
        out.push_str(head);
        out.push(':');
        let after = &tail[1..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        // Judge the path against the whole text already emitted, not just
        // this segment, so a path split by an earlier citation still matches.
        if digits > 0 && ends_with_test_file(&out[..out.len() - 1]) {
            out.push_str("<line>");
            rest = &after[digits..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// True when the rest of a pretty-printed member line is exactly an unsigned
/// canonical unsigned integer (no leading zero), with at most one trailing comma. `null`, a quoted string or a
/// malformed number is not masked, so a broken test pointer stays drift.
fn is_unsigned_json_number_member(rest: &str) -> bool {
    let value = rest.trim_end();
    let value = value.strip_suffix(',').unwrap_or(value);
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// In pretty `check.json`, mask `"line": N` when the nearest preceding
/// `"file"` member names a JS/TS test file (#12163). Related-test entries put
/// `file` immediately before `line`; a `line` after any other file, or with no
/// file before it, is compared as written.
fn mask_test_lines_in_check_json(text: &str) -> String {
    let mut last_file_is_test = false;
    let mut out = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        let trimmed = chunk.trim_start();
        // A `file` only governs the `line` in its own object: leaving or
        // entering an object ends it, so a later `line` with no `file` of its
        // own stays compared.
        if trimmed.starts_with('}') || trimmed.trim_end().ends_with('{') {
            last_file_is_test = false;
        }
        if let Some(value) = trimmed.strip_prefix("\"file\": \"") {
            let name = value.split('"').next().unwrap_or("");
            last_file_is_test = ends_with_test_file(name);
        } else if last_file_is_test
            && trimmed
                .strip_prefix("\"line\": ")
                .is_some_and(is_unsigned_json_number_member)
        {
            let indent = &chunk[..chunk.len() - trimmed.len()];
            let comma = if trimmed.trim_end().ends_with(',') {
                ","
            } else {
                ""
            };
            let newline = if chunk.ends_with('\n') { "\n" } else { "" };
            out.push_str(&format!("{indent}\"line\": \"<line>\"{comma}{newline}"));
            continue;
        }
        out.push_str(chunk);
    }
    out
}

fn mask_check_json_test_locations(text: &str) -> String {
    mask_test_lines_in_check_json(&mask_test_file_citations(text))
}

fn mask_refusal_body(body: &str) -> String {
    if let Some(masked) = normalize_seam_count(body) {
        return masked;
    }
    if let Some(masked) = normalize_preview_packet_location(body) {
        return masked;
    }
    for rule in REPAIR_REFUSAL_RULES {
        if body.starts_with(rule.prefix) {
            return format!("{}<reason>", rule.prefix);
        }
    }
    body.to_string()
}

/// Classification goldens pin analyzer findings, not the per-language
/// completeness projection (#11272). The report renderer writes it as one
/// compact member, and later adapters (`source_subject`, canonical next
/// action) may append after it, so measure the member's own JSON value rather
/// than assuming it is last. Dedicated schema tests own the field.
fn strip_language_completeness_member(value: &str) -> String {
    const MARKER: &str = ",\n  \"language_completeness\": ";
    let Some(start) = value.find(MARKER) else {
        return value.to_string();
    };
    let rest = &value[start + MARKER.len()..];
    let mut values = serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
    if !matches!(values.next(), Some(Ok(_))) {
        return value.to_string();
    }
    let end = start + MARKER.len() + values.byte_offset();
    // The renderer's optional members follow the findings array's own newline
    // (`]\n,\n  "member"`); drop it with the member so the root closer keeps
    // the golden's `]\n}` shape.
    let head = &value[..start];
    let head = head.strip_suffix('\n').unwrap_or(head);
    format!("{head}{}", &value[end..])
}

fn normalize_check_json_golden_document(value: &str) -> String {
    split_compact_root_object_closer(&strip_language_completeness_member(
        &strip_canonical_next_action_member(value),
    ))
}

/// Project the checkout's absolute path prefix to the machine-independent
/// `<cwd>/` placeholder in the JSON surface, like
/// [`project_renderer_cwd`] does for human text. The check JSON names the
/// loaded config by its absolute stable spelling (RIPR-SPEC-0255
/// `config_path`), so checked-in goldens must hold the placeholder instead
/// of a machine path (#7536). Windows canonicalization adds the verbatim
/// `//?/` prefix that the lossless stable encoder preserves; that spelling
/// is matched and projected first, then the plain prefix.
fn project_json_cwd_prefix(text: &str) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        // An unreadable working directory leaves the text unprojected, so a
        // comparison fails loudly instead of passing on machine paths.
        return text.to_string();
    };
    let prefix = format!("{}/", normalize_path(&cwd));
    text.replace(&format!("//?/{prefix}"), "<cwd>/")
        .replace(&prefix, "<cwd>/")
}

pub(crate) fn normalize_fixture_json_output(value: &str) -> String {
    normalize_fixture_json_output_for_host(value, cfg!(windows))
}

/// [`normalize_fixture_json_output`] with the host explicit.
///
/// Only a Windows-produced document carries path separators as backslashes
/// (#2337), so only a Windows host projects them to `/`. Elsewhere a JSON
/// `\\` is a literal backslash of source text (an escaped quote `\"`, a
/// `\u{7b}` escape, a `\n` in an asserted string literal) and must reach
/// the golden unchanged: the former blanket `\\` to `/` rewrote
/// `quoted\"marker\"` to `quoted/"marker/"` in checked-in goldens (#11119).
/// On Windows only string values under a path-bearing key (see
/// [`is_path_bearing_json_key`]) have their separators projected; every
/// other string (an asserted `"a\\nb"`, a `\\u{7b}` escape, a `\\"` quote)
/// is source text and stays byte-identical, so the fold no longer guesses
/// from neighbouring characters.
fn normalize_fixture_json_output_for_host(value: &str, windows: bool) -> String {
    let value = normalize_check_json_golden_document(value);
    if !windows {
        return project_json_cwd_prefix(&value);
    }
    // #2556: operate on CHARS, not bytes, so multi-byte UTF-8 survives.
    let chars: Vec<char> = value.chars().collect();
    let mut result = String::with_capacity(value.len());
    let mut current_key = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '"' {
            result.push(chars[i]);
            i += 1;
            continue;
        }
        // Scan one JSON string token, honoring escapes.
        let start = i;
        i += 1;
        while i < chars.len() && chars[i] != '"' {
            i += if chars[i] == '\\' { 2 } else { 1 };
        }
        let end = i.min(chars.len().saturating_sub(1));
        let token: String = chars[start..=end].iter().collect();
        i = end + 1;
        let mut after = i;
        while chars.get(after).is_some_and(|c| c.is_whitespace()) {
            after += 1;
        }
        if chars.get(after) == Some(&':') {
            current_key = token.trim_matches('"').to_string();
            result.push_str(&token);
        } else if is_path_bearing_json_key(&current_key) {
            result.push_str(&token.replace("\\\\", "/"));
        } else {
            result.push_str(&token);
        }
    }
    project_json_cwd_prefix(&result)
}

/// Keys whose string values are filesystem paths in the check/gate JSON.
///
/// `evidence_path` names the reach/infect/propagate/observe trail, not a
/// filesystem path: its strings quote asserted source text (`\"`, `\u{..}`),
/// so matching its `path` substring would fold source backslashes on Windows
/// and fork the goldens between hosts (#11119).
fn is_path_bearing_json_key(key: &str) -> bool {
    key != "evidence_path"
        && ["path", "file", "root", "dir"]
            .iter()
            .any(|needle| key.contains(needle))
}

/// Human fixture text is never separator-folded: the Windows-facing human
/// producers already print paths with `/` (`human_path`, `display_path`,
/// `root_path_display`), and everything else in it is quoted SOURCE text
/// (`read \"error\"`, `\u{7b}`, `root: \u{7b}`) that must stay byte-identical
/// on every host so the checked-in goldens hold the real source (#11178).
pub(crate) fn normalize_fixture_human_output(value: &str) -> String {
    let normalized = project_renderer_cwd(value);
    let trimmed = normalized.trim_end_matches(['\r', '\n']);
    let mut output = trimmed.to_string();
    output.push('\n');
    output
}

/// The `check` drill-in commands bind `--root` to the resolved repository
/// (#3948), so a rendered fixture carries the renderer's working directory.
/// Checked-in goldens hold the machine-independent `<cwd>/` placeholder
/// instead, the same projection the library tests use.
fn project_renderer_cwd(text: &str) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        // An unreadable working directory leaves the text unprojected, so a
        // comparison fails loudly instead of passing on machine paths.
        return text.to_string();
    };
    project_cwd_prefix(text, &format!("{}/", normalize_path(&cwd)))
}

/// Replace `prefix` with `<cwd>/`. A checkout path that needs shell quoting
/// (a space, `'`, ...) renders the whole token quoted, `'<prefix>tail'`; the
/// goldens hold the form a quote-free checkout renders, so the quotes are
/// dropped whenever the tail alone would not need them.
fn project_cwd_prefix(text: &str, prefix: &str) -> String {
    let quoted_open = format!("'{}", prefix.replace('\'', r"'\''"));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(quoted_open.as_str()) {
        let after = &rest[at + quoted_open.len()..];
        let Some((tail, consumed)) = quoted_tail(after) else {
            break;
        };
        out.push_str(&rest[..at]);
        if !tail.is_empty() && tail.chars().all(is_bare_shell_char) {
            out.push_str("<cwd>/");
            out.push_str(&tail);
        } else {
            out.push_str("'<cwd>/");
            out.push_str(&tail.replace('\'', r"'\''"));
            out.push('\'');
        }
        rest = &after[consumed..];
    }
    out.push_str(rest);
    out.replace(prefix, "<cwd>/")
}

/// Read the rest of a `shell_arg`-quoted token after its opening quote:
/// the unescaped tail and the bytes consumed through the closing quote. An
/// embedded quote renders as `'\''` and stays part of the token (#6762).
fn quoted_tail(after: &str) -> Option<(String, usize)> {
    let mut tail = String::new();
    let mut at = 0;
    loop {
        let end = at + after[at..].find('\'')?;
        tail.push_str(&after[at..end]);
        if after[end..].starts_with(r"'\''") {
            tail.push('\'');
            at = end + 4;
        } else {
            return Some((tail, end + 1));
        }
    }
}

/// Mirror of `shell_arg`'s bare-token rule.
fn is_bare_shell_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':')
}

fn fixture_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("invalid fixture path {}", normalize_path(path)))
}

fn fixture_report_body(
    name: Option<&str>,
    selected: &[PathBuf],
    runs: &[FixtureRun],
    violations: &[String],
) -> String {
    let status = if violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let mut body = format!("# ripr fixtures report\n\nStatus: {status}\n\n");
    match name {
        Some(value) => body.push_str(&format!("Requested fixture: `{value}`\n\n")),
        None => body.push_str("Requested fixture: all fixtures\n\n"),
    }
    body.push_str("## Fixtures\n\n");
    if selected.is_empty() {
        body.push_str("No fixture directories found.\n\n");
    } else {
        for path in selected {
            body.push_str(&format!("- `{}`\n", normalize_path(path)));
        }
        body.push('\n');
    }
    write_fixture_runs_section(&mut body, runs);
    write_violations_section(&mut body, violations);
    body
}

fn goldens_check_report_body(
    fixtures: &[PathBuf],
    runs: &[FixtureRun],
    violations: &[String],
) -> String {
    let status = if violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let mut body = format!("# ripr goldens report\n\nStatus: {status}\n\n");
    body.push_str("## Fixtures\n\n");
    if fixtures.is_empty() {
        body.push_str("No fixture directories found.\n\n");
    } else {
        for fixture in fixtures {
            body.push_str(&format!("- `{}`\n", normalize_path(fixture)));
        }
        body.push('\n');
    }
    write_fixture_runs_section(&mut body, runs);
    write_violations_section(&mut body, violations);
    body
}

/// Build identity for the actual side of a golden drift report (#7401).
/// `goldens check` compares committed `expected/` bytes against bytes a
/// freshly built binary just produced, but neither the console failure card
/// nor the drift report recorded which build the actual side came from — a
/// drift entry could not be tied to the binary that generated it. Every
/// field fails soft to an explicit `unavailable` note: identity here is
/// disclosure, not gate authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GoldenDriftProvenance {
    /// The worktree-absolute debug binary every fixture spawn used.
    pub(crate) binary: String,
    /// Its `ripr --version` line: `ripr <version> (<commit>[-dirty])`.
    pub(crate) ripr_version: String,
    /// `git rev-parse HEAD` of the checkout the report ran in.
    pub(crate) revision: String,
}

fn golden_drift_provenance() -> GoldenDriftProvenance {
    let binary = ripr_fixture_binary().unwrap_or_else(|_| "unavailable".to_string());
    let ripr_version = if binary == "unavailable" {
        "unavailable".to_string()
    } else {
        run_output_optional(&binary, &["--version"])
            .ok()
            .and_then(|text| {
                text.lines()
                    .next()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "unavailable".to_string())
    };
    let revision = run_output_optional("git", &["rev-parse", "HEAD"])
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unavailable".to_string());
    GoldenDriftProvenance {
        binary,
        ripr_version,
        revision,
    }
}

fn write_golden_drift_reports(
    runs: &[FixtureRun],
    violations: &[String],
    provenance: &GoldenDriftProvenance,
) -> Result<Vec<GoldenDriftEntry>, String> {
    let changed_paths = collect_changed_paths_set().unwrap_or_default();
    let entries = golden_drift_entries(runs, &changed_paths)?;
    let markdown = golden_drift_markdown(&entries, violations, provenance);
    let json = golden_drift_json(&entries, violations, provenance);
    write_report("golden-drift.md", &markdown)?;
    write_report("golden-drift.json", &json)?;
    Ok(entries)
}

/// Summarize a single golden drift as a semantic category, so a `goldens check`
/// failure says *what kind* of drift happened (classification flip, added/removed
/// finding, oracle change, banned static-language term, or formatting-only) rather
/// than only pointing at a report file. Reuses the already-computed
/// [`GoldenDriftSemantics`]; this is the discriminator a reviewer needs to decide
/// whether a flip is intended (re-bless) or a regression.
pub(crate) fn golden_drift_type(semantics: &GoldenDriftSemantics) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !semantics.changed_exposure_classes.is_empty() {
        parts.push(format!(
            "classification_changed [{}]",
            semantics.changed_exposure_classes.join("; ")
        ));
    }
    if !semantics.added_finding_ids.is_empty() {
        parts.push(format!(
            "finding_added x{}",
            semantics.added_finding_ids.len()
        ));
    }
    if !semantics.removed_finding_ids.is_empty() {
        parts.push(format!(
            "finding_removed x{}",
            semantics.removed_finding_ids.len()
        ));
    }
    if !semantics.changed_oracle_strengths.is_empty() {
        parts.push("oracle_strength_changed".to_string());
    }
    if !semantics.changed_oracle_kinds.is_empty() {
        parts.push("oracle_kind_changed".to_string());
    }
    if !semantics.changed_probe_families.is_empty() {
        parts.push("probe_family_changed".to_string());
    }
    if !semantics.changed_stop_reasons.is_empty() {
        parts.push("stop_reason_changed".to_string());
    }
    if !semantics.changed_recommendations.is_empty() {
        parts.push("recommendation_changed".to_string());
    }
    if !semantics.static_language_terms.is_empty() {
        parts.push(format!(
            "banned_static_language [{}]",
            semantics.static_language_terms.join(", ")
        ));
    }
    if parts.is_empty() {
        parts.push(format!(
            "formatting_only ({} line(s))",
            semantics.changed_line_count
        ));
    }
    parts.join(", ")
}

fn golden_drift_entries(
    runs: &[FixtureRun],
    changed_paths: &BTreeSet<String>,
) -> Result<Vec<GoldenDriftEntry>, String> {
    let mut entries = Vec::new();
    for run in runs {
        let changelog = Path::new("fixtures")
            .join(&run.name)
            .join("expected")
            .join("CHANGELOG.md");
        let blessing_reason_present = changed_paths.contains(&normalize_path(&changelog));
        for comparison in &run.comparisons {
            if comparison.matches {
                continue;
            }
            let expected = read_text_lossy(&comparison.expected)?;
            let actual = read_text_lossy(&comparison.actual)?;
            entries.push(GoldenDriftEntry {
                fixture: run.name.clone(),
                surface: comparison.surface.to_string(),
                expected: normalize_path(&comparison.expected),
                actual: normalize_path(&comparison.actual),
                blessing_reason_required: true,
                blessing_reason_present,
                semantics: golden_drift_semantics(comparison.surface, &expected, &actual),
            });
        }
    }
    Ok(entries)
}

pub(crate) fn golden_drift_semantics(
    surface: &str,
    expected: &str,
    actual: &str,
) -> GoldenDriftSemantics {
    let changed_line_count = changed_line_count(expected, actual);
    let static_language_terms = static_language_terms(expected, actual);
    if surface == "check.json" {
        let expected_ids = json_string_values_for_key(expected, "id")
            .into_iter()
            .filter(|value| value.starts_with("probe:"))
            .collect::<BTreeSet<_>>();
        let actual_ids = json_string_values_for_key(actual, "id")
            .into_iter()
            .filter(|value| value.starts_with("probe:"))
            .collect::<BTreeSet<_>>();
        let expected_classes = json_string_values_for_key(expected, "classification");
        let actual_classes = json_string_values_for_key(actual, "classification");
        let expected_families = json_string_values_for_key(expected, "family");
        let actual_families = json_string_values_for_key(actual, "family");
        let expected_oracles = json_string_values_for_key(expected, "oracle_strength");
        let actual_oracles = json_string_values_for_key(actual, "oracle_strength");
        let expected_oracle_kinds = json_string_values_for_key(expected, "oracle_kind");
        let actual_oracle_kinds = json_string_values_for_key(actual, "oracle_kind");
        let expected_stop_reasons = json_stop_reason_values(expected);
        let actual_stop_reasons = json_stop_reason_values(actual);
        let expected_recommendations =
            json_string_values_for_key(expected, "recommended_next_step");
        let actual_recommendations = json_string_values_for_key(actual, "recommended_next_step");

        GoldenDriftSemantics {
            added_finding_ids: set_difference(&actual_ids, &expected_ids),
            removed_finding_ids: set_difference(&expected_ids, &actual_ids),
            changed_exposure_classes: set_change_summary(&expected_classes, &actual_classes),
            changed_probe_families: set_change_summary(&expected_families, &actual_families),
            changed_oracle_kinds: set_change_summary(&expected_oracle_kinds, &actual_oracle_kinds),
            changed_oracle_strengths: set_change_summary(&expected_oracles, &actual_oracles),
            changed_stop_reasons: set_change_summary(&expected_stop_reasons, &actual_stop_reasons),
            changed_recommendations: set_change_summary(
                &expected_recommendations,
                &actual_recommendations,
            ),
            static_language_terms,
            changed_line_count,
        }
    } else {
        let expected_recommendations = human_recommended_next_steps(expected);
        let actual_recommendations = human_recommended_next_steps(actual);
        let expected_stop_reasons = human_stop_reason_lines(expected);
        let actual_stop_reasons = human_stop_reason_lines(actual);
        GoldenDriftSemantics {
            changed_stop_reasons: set_change_summary(&expected_stop_reasons, &actual_stop_reasons),
            changed_recommendations: set_change_summary(
                &expected_recommendations,
                &actual_recommendations,
            ),
            static_language_terms,
            changed_line_count,
            ..GoldenDriftSemantics::default()
        }
    }
}

fn golden_drift_markdown(
    entries: &[GoldenDriftEntry],
    violations: &[String],
    provenance: &GoldenDriftProvenance,
) -> String {
    let status = if violations
        .iter()
        .any(|violation| !violation.contains("drift for fixture"))
    {
        "fail"
    } else if entries.is_empty() {
        "pass"
    } else {
        "warn"
    };
    let mut body = format!("# ripr golden drift report\n\nStatus: {status}\n\n");
    body.push_str("This report summarizes expected-output drift for reviewer inspection. It never blesses goldens.\n\n");
    body.push_str("## Provenance\n\n");
    body.push_str(&format!("- binary: `{}`\n", provenance.binary));
    body.push_str(&format!("- ripr version: `{}`\n", provenance.ripr_version));
    body.push_str(&format!("- revision: `{}`\n", provenance.revision));
    body.push_str("\n## Summary\n\n");
    body.push_str(&format!("- drift entries: {}\n", entries.len()));
    body.push_str(&format!(
        "- fixture errors: {}\n",
        fixture_error_count(violations)
    ));
    body.push_str("\n## Drift\n\n");
    if entries.is_empty() {
        body.push_str("None detected.\n\n");
    } else {
        for entry in entries {
            body.push_str(&format!("### `{}` `{}`\n\n", entry.fixture, entry.surface));
            body.push_str(&format!("- expected: `{}`\n", entry.expected));
            body.push_str(&format!("- actual: `{}`\n", entry.actual));
            body.push_str(&format!(
                "- changed lines: {}\n",
                entry.semantics.changed_line_count
            ));
            write_optional_list(
                &mut body,
                "added finding IDs",
                &entry.semantics.added_finding_ids,
            );
            write_optional_list(
                &mut body,
                "removed finding IDs",
                &entry.semantics.removed_finding_ids,
            );
            write_optional_list(
                &mut body,
                "changed exposure classes",
                &entry.semantics.changed_exposure_classes,
            );
            write_optional_list(
                &mut body,
                "changed probe families",
                &entry.semantics.changed_probe_families,
            );
            write_optional_list(
                &mut body,
                "changed oracle strengths",
                &entry.semantics.changed_oracle_strengths,
            );
            write_optional_list(
                &mut body,
                "changed oracle kinds",
                &entry.semantics.changed_oracle_kinds,
            );
            write_optional_list(
                &mut body,
                "changed stop reasons",
                &entry.semantics.changed_stop_reasons,
            );
            write_optional_list(
                &mut body,
                "changed recommended next steps",
                &entry.semantics.changed_recommendations,
            );
            if entry.semantics.static_language_terms.is_empty() {
                body.push_str("- static-language boundary: pass\n");
            } else {
                body.push_str("- static-language boundary: fail\n");
                write_optional_list(
                    &mut body,
                    "static-language terms",
                    &entry.semantics.static_language_terms,
                );
            }
            body.push_str(&format!(
                "- blessing reason required: {}\n",
                yes_no(entry.blessing_reason_required)
            ));
            body.push_str(&format!(
                "- blessing reason present in PR: {}\n\n",
                yes_no(entry.blessing_reason_present)
            ));
        }
    }
    if !violations.is_empty() {
        body.push_str("## Fixture / Golden Violations\n\n");
        write_violations_section(&mut body, violations);
    }
    body
}

fn golden_drift_json(
    entries: &[GoldenDriftEntry],
    violations: &[String],
    provenance: &GoldenDriftProvenance,
) -> String {
    let status = if violations
        .iter()
        .any(|violation| !violation.contains("drift for fixture"))
    {
        "fail"
    } else if entries.is_empty() {
        "pass"
    } else {
        "warn"
    };
    let mut body = String::from("{\n");
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str(&format!("  \"status\": \"{}\",\n", json_escape(status)));
    body.push_str(&format!("  \"drift_count\": {},\n", entries.len()));
    body.push_str("  \"provenance\": {\n");
    body.push_str(&format!(
        "    \"binary\": \"{}\",\n",
        json_escape(&provenance.binary)
    ));
    body.push_str(&format!(
        "    \"ripr_version\": \"{}\",\n",
        json_escape(&provenance.ripr_version)
    ));
    body.push_str(&format!(
        "    \"revision\": \"{}\"\n",
        json_escape(&provenance.revision)
    ));
    body.push_str("  },\n");
    body.push_str("  \"entries\": [\n");
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!(
            "      \"fixture\": \"{}\",\n",
            json_escape(&entry.fixture)
        ));
        body.push_str(&format!(
            "      \"surface\": \"{}\",\n",
            json_escape(&entry.surface)
        ));
        body.push_str(&format!(
            "      \"expected\": \"{}\",\n",
            json_escape(&entry.expected)
        ));
        body.push_str(&format!(
            "      \"actual\": \"{}\",\n",
            json_escape(&entry.actual)
        ));
        // Each entry carries the build identity too, so a copied or
        // retained single entry stays tied to the binary that produced its
        // actual side (#7401).
        body.push_str("      \"provenance\": {\n");
        body.push_str(&format!(
            "        \"binary\": \"{}\",\n",
            json_escape(&provenance.binary)
        ));
        body.push_str(&format!(
            "        \"ripr_version\": \"{}\",\n",
            json_escape(&provenance.ripr_version)
        ));
        body.push_str(&format!(
            "        \"revision\": \"{}\"\n",
            json_escape(&provenance.revision)
        ));
        body.push_str("      },\n");
        body.push_str(&format!(
            "      \"changed_line_count\": {},\n",
            entry.semantics.changed_line_count
        ));
        body.push_str(&format!(
            "      \"blessing_reason_required\": {},\n",
            entry.blessing_reason_required
        ));
        body.push_str(&format!(
            "      \"blessing_reason_present\": {},\n",
            entry.blessing_reason_present
        ));
        write_json_field_array(
            &mut body,
            "added_finding_ids",
            &entry.semantics.added_finding_ids,
            true,
        );
        write_json_field_array(
            &mut body,
            "removed_finding_ids",
            &entry.semantics.removed_finding_ids,
            true,
        );
        write_json_field_array(
            &mut body,
            "changed_exposure_classes",
            &entry.semantics.changed_exposure_classes,
            true,
        );
        write_json_field_array(
            &mut body,
            "changed_probe_families",
            &entry.semantics.changed_probe_families,
            true,
        );
        write_json_field_array(
            &mut body,
            "changed_oracle_strengths",
            &entry.semantics.changed_oracle_strengths,
            true,
        );
        write_json_field_array(
            &mut body,
            "changed_oracle_kinds",
            &entry.semantics.changed_oracle_kinds,
            true,
        );
        write_json_field_array(
            &mut body,
            "changed_stop_reasons",
            &entry.semantics.changed_stop_reasons,
            true,
        );
        write_json_field_array(
            &mut body,
            "changed_recommendations",
            &entry.semantics.changed_recommendations,
            true,
        );
        write_json_field_array(
            &mut body,
            "static_language_terms",
            &entry.semantics.static_language_terms,
            false,
        );
        body.push_str("\n    }");
    }
    body.push_str("\n  ],\n");
    body.push_str("  \"violations\": [");
    write_json_string_array(&mut body, violations);
    body.push_str("]\n");
    body.push_str("}\n");
    body
}

fn write_fixture_runs_section(body: &mut String, runs: &[FixtureRun]) {
    body.push_str("## Actual Outputs\n\n");
    if runs.is_empty() {
        body.push_str("No fixture outputs generated.\n\n");
        return;
    }
    for run in runs {
        body.push_str(&format!(
            "- `{}` -> `{}`\n",
            run.name,
            normalize_path(&run.actual_dir)
        ));
        body.push_str(&format!("  - `{}`\n", normalize_path(&run.check_json)));
        body.push_str(&format!("  - `{}`\n", normalize_path(&run.human_txt)));
    }
    body.push('\n');

    body.push_str("## Golden Comparisons\n\n");
    for run in runs {
        if run.comparisons.is_empty() {
            body.push_str(&format!(
                "- `{}`: no expected outputs compared.\n",
                run.name
            ));
            continue;
        }
        for comparison in &run.comparisons {
            let status = if comparison.matches { "pass" } else { "fail" };
            body.push_str(&format!(
                "- `{}` `{}`: {status}\n  - expected: `{}`\n  - actual: `{}`\n",
                run.name,
                comparison.surface,
                normalize_path(&comparison.expected),
                normalize_path(&comparison.actual)
            ));
        }
    }
    body.push('\n');
}

fn write_optional_list(body: &mut String, label: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    body.push_str(&format!("- {label}:\n"));
    for value in values {
        body.push_str(&format!("  - `{}`\n", markdown_cell(value)));
    }
}

fn write_json_field_array(body: &mut String, key: &str, values: &[String], trailing_comma: bool) {
    body.push_str(&format!("      \"{}\": [", json_escape(key)));
    write_json_string_array(body, values);
    body.push(']');
    if trailing_comma {
        body.push(',');
    }
    body.push('\n');
}

pub(crate) fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn fixture_error_count(violations: &[String]) -> usize {
    violations
        .iter()
        .filter(|violation| !violation.contains("drift for fixture"))
        .count()
}

fn changed_line_count(expected: &str, actual: &str) -> usize {
    let expected_lines = normalize_golden_text(expected)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let actual_lines = normalize_golden_text(actual)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let max_len = expected_lines.len().max(actual_lines.len());
    let mut changed = 0usize;
    for index in 0..max_len {
        if expected_lines.get(index) != actual_lines.get(index) {
            changed += 1;
        }
    }
    changed
}

fn static_language_terms(expected: &str, actual: &str) -> Vec<String> {
    let combined = format!("{expected}\n{actual}").to_ascii_lowercase();
    forbidden_static_terms()
        .into_iter()
        .filter(|term| contains_word(&combined, term))
        .collect()
}

fn set_difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

fn set_change_summary(expected: &BTreeSet<String>, actual: &BTreeSet<String>) -> Vec<String> {
    if expected == actual {
        Vec::new()
    } else {
        vec![format!(
            "expected [{}] -> actual [{}]",
            expected.iter().cloned().collect::<Vec<_>>().join(", "),
            actual.iter().cloned().collect::<Vec<_>>().join(", ")
        )]
    }
}

fn json_stop_reason_values(text: &str) -> BTreeSet<String> {
    let mut values = json_string_values_for_key(text, "stop_reason");
    values.extend(json_string_values_for_key(text, "stop_reasons"));
    values
}

pub(crate) fn json_string_values_for_key(text: &str, key: &str) -> BTreeSet<String> {
    let needle = format!("\"{key}\"");
    let mut values = BTreeSet::new();
    let mut multiline = String::new();
    let mut collecting = false;
    for line in text.lines() {
        if collecting {
            multiline.push(' ');
            multiline.push_str(line.trim());
            if line.contains(']') {
                values.extend(json_strings_in_fragment(&multiline));
                multiline.clear();
                collecting = false;
            }
            continue;
        }
        let Some((_, rest)) = line.split_once(&needle) else {
            continue;
        };
        let Some((_, value)) = rest.split_once(':') else {
            continue;
        };
        let trimmed = value.trim();
        if trimmed.starts_with('[') && !trimmed.contains(']') {
            multiline.push_str(trimmed);
            collecting = true;
        } else {
            values.extend(json_strings_in_fragment(trimmed));
        }
    }
    values
}

fn json_strings_in_fragment(value: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut chars = value.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut current = String::new();
        let mut escaped = false;
        for (_, next) in chars.by_ref() {
            if escaped {
                current.push(match next {
                    '"' => '"',
                    '\\' => '\\',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
                continue;
            }
            if next == '\\' {
                escaped = true;
                continue;
            }
            if next == '"' {
                strings.push(current);
                break;
            }
            current.push(next);
        }
    }
    strings
}

fn human_recommended_next_steps(text: &str) -> BTreeSet<String> {
    human_section_lines(text, "Recommended next step:")
}

fn human_stop_reason_lines(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("stop reason") || lower.contains("stop:")
        })
        .map(str::to_string)
        .collect()
}

fn human_section_lines(text: &str, heading: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    let mut capture = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if capture {
            if trimmed.is_empty() {
                capture = false;
            } else {
                values.insert(trimmed.to_string());
            }
            continue;
        }
        if trimmed == heading {
            capture = true;
        }
    }
    values
}

fn collect_changed_paths_set() -> Result<BTreeSet<String>, String> {
    Ok(collect_pr_changes()?
        .into_iter()
        .map(|change| change.path)
        .collect())
}

fn write_violations_section(body: &mut String, violations: &[String]) {
    body.push_str("## Violations\n\n");
    if violations.is_empty() {
        body.push_str("None detected.\n");
    } else {
        for violation in violations {
            body.push_str("```text\n");
            body.push_str(violation);
            body.push_str("\n```\n\n");
        }
    }
}

pub(crate) fn parse_reason(args: &[String]) -> Result<String, String> {
    let mut index = 0;
    while index < args.len() {
        let value = &args[index];
        if let Some(reason) = value.strip_prefix("--reason=") {
            return non_empty_reason(reason);
        }
        if value == "--reason" {
            let Some(reason) = args.get(index + 1) else {
                return Err("--reason requires a value".to_string());
            };
            return non_empty_reason(reason);
        }
        index += 1;
    }
    Err("goldens bless requires --reason <reason>".to_string())
}

fn non_empty_reason(value: &str) -> Result<String, String> {
    let reason = value.trim();
    if reason.is_empty() {
        return Err("--reason must not be empty".to_string());
    }
    if spec_ids_in_reason(reason).is_empty() {
        return Err(format!(
            "--reason must cite a spec id (e.g. RIPR-SPEC-0001: output contract changed); got: {reason:?}"
        ));
    }
    Ok(reason.to_string())
}

pub(crate) fn validate_bless_reason(reason: &str) -> Result<(), String> {
    let referenced = spec_ids_in_reason(reason);
    if referenced.is_empty() {
        return Err(format!(
            "--reason must cite a spec id (e.g. RIPR-SPEC-0001: output contract changed); got: {reason:?}"
        ));
    }
    let known = known_spec_ids()?;
    let unknown: Vec<_> = referenced.difference(&known).cloned().collect();
    if !unknown.is_empty() {
        return Err(format!(
            "--reason cites unknown spec id(s): {}; use an existing docs/specs/RIPR-SPEC-NNNN-*.md id",
            unknown.join(", ")
        ));
    }
    Ok(())
}

fn spec_ids_in_reason(value: &str) -> BTreeSet<String> {
    const PREFIX: &str = "RIPR-SPEC-";
    let mut ids = BTreeSet::new();
    for (offset, _) in value.match_indices(PREFIX) {
        let suffix = &value[offset + PREFIX.len()..];
        let digits: String = suffix.chars().take(4).collect();
        if digits.len() != 4 || !digits.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        if suffix
            .chars()
            .nth(4)
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            continue;
        }
        ids.insert(format!("{PREFIX}{digits}"));
    }
    ids
}

fn known_spec_ids() -> Result<BTreeSet<String>, String> {
    let specs_dir = repository_specs_dir()?;
    let entries = fs::read_dir(&specs_dir)
        .map_err(|err| format!("failed to read {}: {err}", normalize_path(&specs_dir)))?;
    let mut ids = BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|err| format!("failed to read spec entry: {err}"))?
            .path();
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let mut parts = stem.splitn(4, '-');
        if parts.next() != Some("RIPR")
            || parts.next() != Some("SPEC")
            || parts.next().is_none_or(|value| {
                value.len() != 4 || !value.chars().all(|character| character.is_ascii_digit())
            })
        {
            continue;
        }
        let Some(number) = stem.split('-').nth(2) else {
            continue;
        };
        ids.insert(format!("RIPR-SPEC-{number}"));
    }
    Ok(ids)
}

fn repository_specs_dir() -> Result<PathBuf, String> {
    let mut directory = std::env::current_dir()
        .map_err(|err| format!("failed to determine current directory: {err}"))?;
    loop {
        let candidate = directory.join("docs").join("specs");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        if !directory.pop() {
            return Err("could not locate repository docs/specs directory".to_string());
        }
    }
}

pub(crate) use self::fixtures_impl as fixtures;
pub(crate) use self::golden_drift_impl as golden_drift;
pub(crate) use self::goldens_impl as goldens;

/// Route `fixtures` subcommands: `new <name>` scaffolds, everything else runs (#2445).
pub(crate) fn fixtures_with_args(args: &[String]) -> Result<(), String> {
    if let Some(sub) = args.first().map(String::as_str) {
        if sub == "--help" || sub == "-h" {
            println!(
                "Usage: cargo xtask fixtures [name] — run fixture validation for one fixture or all"
            );
            println!(
                "       cargo xtask fixtures new <name> — scaffold a new fixture directory with a seeded golden (#2445)"
            );
            println!(
                "       cargo xtask fixtures refresh-identity <typescript|python> — re-derive installed-journey digests, snapshot ids and corpus bindings after a dependency bump (#9261)"
            );
            println!();
            println!("Without a name, runs all fixtures. With a name, runs only that fixture.");
            println!("Use `new <name>` to scaffold a new fixture directory.");
            return Ok(());
        }
        if sub == "new" {
            let name = args
                .get(1)
                .ok_or_else(|| "fixtures new requires a <name> argument".to_string())?;
            return fixtures_new(name);
        }
        if sub == "refresh-identity" {
            return crate::fixture_contracts::fixtures_refresh_identity(&args[1..]);
        }
    }
    // Fall through to the existing runner (pass the first arg as the optional fixture name).
    fixtures_impl(args.first())
}

/// Live spec id cited by the `fixtures new` step-4 bless command (#7398).
/// `goldens bless` rejects reasons that do not cite an existing spec; this id
/// exists in `docs/specs/` so the printed command is copy-pasteable. Replace it
/// with the spec the new fixture proves (`cargo xtask specs next` allocates).
const FIXTURES_NEW_BLESS_EXAMPLE_SPEC: &str = "RIPR-SPEC-0001";

fn fixtures_new_bless_reason(spec_id: &str) -> String {
    format!("{spec_id}: initial scaffold")
}

/// Step 4 of the `fixtures new` next-actions list. Kept as one production
/// string so the printed command and the reason-validator test cannot drift.
fn fixtures_new_step4(name: &str) -> String {
    let reason = fixtures_new_bless_reason(FIXTURES_NEW_BLESS_EXAMPLE_SPEC);
    format!(
        "  4. Run: cargo xtask goldens bless {name} --reason \"{reason}\"\n     Replace {FIXTURES_NEW_BLESS_EXAMPLE_SPEC} with the spec this fixture proves (`cargo xtask specs next` to allocate). Bless rejects reasons that do not cite an existing spec id."
    )
}

/// Scaffold a new fixture directory with a seeded golden (#2445).
fn fixtures_new(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(format!(
            "fixture name must be a simple directory name (no path separators): {name:?}"
        ));
    }
    let fixture_dir = Path::new("fixtures").join(name);
    if fixture_dir.exists() {
        return Err(format!("fixture already exists: {}", fixture_dir.display()));
    }

    // Create the directory tree.
    let input_dir = fixture_dir.join("input");
    let src_dir = input_dir.join("src");
    let expected_dir = fixture_dir.join("expected");
    std::fs::create_dir_all(&src_dir).map_err(|err| format!("create {src_dir:?}: {err}"))?;
    std::fs::create_dir_all(&expected_dir)
        .map_err(|err| format!("create {expected_dir:?}: {err}"))?;

    // Write SPEC.md with BDD structure.
    let spec = format!(
        "# Fixture: {name}\n\n\
         Spec: RIPR-SPEC-NNNN\n\n\
         ## Given\n\n\
         (Describe the input code and test state.)\n\n\
         ## When\n\n\
         (Describe the diff that changes behavior.)\n\n\
         ## Then\n\n\
         (Describe the expected ripr findings.)\n\n\
         ## Must Not\n\n\
         (Describe what ripr must NOT report.)\n"
    );
    std::fs::write(fixture_dir.join("SPEC.md"), spec)
        .map_err(|err| format!("write SPEC.md: {err}"))?;

    // Write a minimal input workspace.
    let cargo_toml =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n");
    std::fs::write(input_dir.join("Cargo.toml"), cargo_toml)
        .map_err(|err| format!("write Cargo.toml: {err}"))?;

    let lib_rs = "// Edit this file to set up the input source code.\n\
                  pub fn example() -> i32 { 42 }\n";
    std::fs::write(src_dir.join("lib.rs"), lib_rs).map_err(|err| format!("write lib.rs: {err}"))?;

    // Write a minimal diff.patch.
    let diff = "--- Edit this diff to describe the behavior change.\n\
                diff --git a/src/lib.rs b/src/lib.rs\n\
                --- a/src/lib.rs\n+++ b/src/lib.rs\n\
                @@ -1,1 +1,1 @@\n\
                -pub fn example() -> i32 { 42 }\n\
                +pub fn example() -> i32 { 84 }\n";
    std::fs::write(fixture_dir.join("diff.patch"), diff)
        .map_err(|err| format!("write diff.patch: {err}"))?;

    // Seed expected/check.json by running ripr check once.
    let run = run_fixture_outputs(&fixture_dir);
    match run {
        Ok(run) => {
            // Copy the actual output to expected.
            std::fs::copy(&run.check_json, expected_dir.join("check.json"))
                .map_err(|err| format!("seed check.json: {err}"))?;
            if let Ok(human) = std::fs::read_to_string(&run.human_txt) {
                std::fs::write(expected_dir.join("human.txt"), human)
                    .map_err(|err| format!("seed human.txt: {err}"))?;
            }
        }
        Err(err) => {
            // Seeding failed — the fixture still has SPEC.md, input/, and diff.patch.
            // The contributor can debug and run `cargo xtask goldens bless <name>` later.
            eprintln!(
                "warning: could not seed golden (the fixture is created but expected/ may be empty): {err}"
            );
        }
    }

    println!("Created fixture: {}", fixture_dir.display());
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {}/input/src/lib.rs to set up the source code.",
        name
    );
    println!(
        "  2. Edit {}/diff.patch to describe the behavior change.",
        name
    );
    println!(
        "  3. Edit {}/SPEC.md to fill in the Given/When/Then/Must Not, and replace the\n     `Spec: RIPR-SPEC-NNNN` placeholder with a real id (`cargo xtask specs next`\n     allocates one; or cite an existing docs/specs id). `fixtures` rejects the placeholder.",
        name
    );
    println!("{}", fixtures_new_step4(name));
    println!("  5. Run: cargo xtask fixtures {} to verify.", name);

    Ok(())
}
#[cfg(test)]
mod tests {

    #[test]
    fn isolated_default_target_sits_beside_the_shared_debug_dir_not_inside_it() {
        let shared = std::path::Path::new("/w/target/debug/ripr");
        // `std::path::absolute` anchors a rootless Windows path to the current
        // drive (`/w` gains a drive prefix), so the expectation goes through it too.
        assert_eq!(
            super::isolated_default_target_dir(shared),
            std::path::absolute("/w/target/fixture-variants/default")
                .map_err(|err| format!("resolve isolated default target: {err}"))
        );
        let missing = super::isolated_default_target_dir(std::path::Path::new("ripr"));
        assert_eq!(missing, Err("missing shared target root".to_string()));
    }

    #[test]
    fn windows_python_golden_projection_keeps_fixture_and_selector_identity() {
        let bash = "  Verify command (Bash): (cd -P -- <cwd>/fixtures/py/input && python -m pytest tests/test_a.py::test_one) (high)\n";
        let manual = "  Suggested command (unverified; inspect and run from the selected root): python -m pytest tests/test_a.py::test_one\n";
        assert_eq!(
            super::project_windows_python_verify_expected("py", bash, true),
            manual
        );
        assert_eq!(
            super::project_windows_python_verify_expected("py", bash, false),
            bash
        );
        assert_eq!(
            super::project_windows_python_verify_expected("other", bash, true),
            bash
        );
        let hostile = bash.replace("test_one", "$(touch x)");
        assert_eq!(
            super::project_windows_python_verify_expected("py", &hostile, true),
            hostile
        );
        let other = bash.replace("test_one", "test_two");
        assert_ne!(
            super::project_windows_python_verify_expected("py", &other, true),
            manual
        );
        let full = bash.replace("Verify command (Bash)", "verify (Bash)");
        assert_eq!(
            super::project_windows_python_verify_expected("py", &full, true),
            manual.replace("Suggested command", "suggested command")
        );
    }

    // #8421: the precommit goldens trigger and gap classification.

    #[test]
    fn staleness_trigger_names_output_surface_and_golden_paths_only() {
        let paths = |items: &[&str]| items.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        for path in [
            "crates/ripr/src/output/human/triage.rs",
            "crates/ripr/src/domain/classification.rs",
            "fixtures/rust_map_overwrite_count/expected/human.txt",
            "docs/OUTPUT_SCHEMA.md",
            "policy/output_contracts.txt",
            "crates/ripr/src/lsp.rs",
        ] {
            let changed = paths(&["README.md", path]);
            assert_eq!(super::golden_staleness_trigger(&changed), Some(path));
        }
        for path in [
            "crates/ripr/src/analysis/probe.rs",
            "docs/ROADMAP.md",
            "docs/golden-guide.md",
            "xtask/src/main.rs",
            "fixtures/rust_map_overwrite_count/input/src/lib.rs",
        ] {
            let changed = paths(&[path]);
            assert_eq!(super::golden_staleness_trigger(&changed), None, "{path}");
        }
        assert_eq!(super::golden_staleness_trigger(&[]), None);
    }

    #[test]
    fn precommit_goldens_treat_only_preparation_failures_as_an_environment_gap() {
        let gap = "fixture dependency preparation failed for owner_return_pin_fallible_literal\nenvironment: X\nerror".to_string();
        let drift = "drift for fixture `rust_map_overwrite_count` [human.txt]".to_string();
        let other = "fixture contract violation".to_string();

        assert_eq!(
            super::classify_precommit_goldens(std::slice::from_ref(&gap)),
            Some(super::PrecommitGoldens::EnvironmentGap(vec![
                "fixture dependency preparation failed for owner_return_pin_fallible_literal"
                    .to_string()
            ]))
        );
        // Drift alongside a gap is still a failure: the gap must not hide it.
        assert_eq!(
            super::classify_precommit_goldens(&[gap.clone(), drift.clone()]),
            None
        );
        assert_eq!(super::classify_precommit_goldens(&[drift]), None);
        assert_eq!(super::classify_precommit_goldens(&[gap, other]), None);
        assert_eq!(super::classify_precommit_goldens(&[]), None);
    }
    fn synthetic_fixture(
        label: &str,
        input_bytes: &[u8],
        embeds: bool,
    ) -> Result<std::path::PathBuf, String> {
        let dir = unique_temp_path(label);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("input/src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dir.join("expected")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("input/src/lib.rs"), input_bytes).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("diff.patch"), "diff\n").map_err(|e| e.to_string())?;
        let check = if embeds {
            "{\"analysis_outcome\":{\"identity\":{\"input_identity\":\"sha256:abc\"}},\"findings\":[]}"
        } else {
            "{\"findings\":[]}"
        };
        std::fs::write(dir.join("expected/check.json"), check).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    fn synthetic_fixture_channels(
        label: &str,
        diff: &[u8],
        config: Option<&[u8]>,
        source: &[u8],
    ) -> Result<std::path::PathBuf, String> {
        let dir = synthetic_fixture(label, source, true)?;
        std::fs::write(dir.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        if let Some(bytes) = config {
            std::fs::write(dir.join("input/ripr.toml"), bytes).map_err(|e| e.to_string())?;
        }
        Ok(dir)
    }

    fn eol_crlf(dir: &std::path::Path, rel: &str) -> BTreeMap<String, GitEolInfo> {
        let mut eol = BTreeMap::new();
        eol.insert(
            normalize_path(&dir.join(rel)),
            GitEolInfo {
                index: "lf".into(),
                worktree: "crlf".into(),
                attrs: "text=auto eol=lf".into(),
            },
        );
        eol
    }

    #[test]
    fn crlf_guard_fires_per_channel_independently() -> Result<(), String> {
        // A: diff.patch only.
        let dir = synthetic_fixture_channels(
            "8614-chan-diff",
            b"diff --git a/x b/x\r\n+fn a() {}\r\n",
            None,
            b"fn a() {}\n",
        )?;
        let v = crlf_guard_violations_with(&dir, &eol_crlf(&dir, "diff.patch"), &BTreeSet::new());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("diff.patch"), "{v:?}");
        // B: loaded config only (input/ripr.toml is the file --root loads first).
        let dir = synthetic_fixture_channels(
            "8614-chan-config",
            b"diff\n",
            Some(b"[languages]\r\nenabled = [\"rust\", \"python\"]\r\n"),
            b"fn a() {}\n",
        )?;
        let v =
            crlf_guard_violations_with(&dir, &eol_crlf(&dir, "input/ripr.toml"), &BTreeSet::new());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("ripr.toml"), "{v:?}");
        // C: source input only (existing lib.rs case, kept as the C pin).
        let dir =
            synthetic_fixture_channels("8614-chan-source", b"diff\n", None, b"fn a() {}\r\n")?;
        let v =
            crlf_guard_violations_with(&dir, &eol_crlf(&dir, "input/src/lib.rs"), &BTreeSet::new());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("lib.rs"), "{v:?}");
        Ok(())
    }

    #[test]
    fn crlf_guard_fires_on_synthetic_crlf_input() -> Result<(), String> {
        // #8614 deterministic control: CRLF bytes + git saying i/lf w/crlf.
        let dir = synthetic_fixture("8614-crlf", b"fn a() {}\r\nfn b() {}\r\n", true)?;
        let key = normalize_path(&dir.join("input/src/lib.rs"));
        let mut eol = BTreeMap::new();
        eol.insert(
            key,
            GitEolInfo {
                index: "lf".into(),
                worktree: "crlf".into(),
                attrs: "text=auto eol=lf".into(),
            },
        );
        let violations = crlf_guard_violations_with(&dir, &eol, &BTreeSet::new());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("lib.rs"), "{violations:?}");
        // Byte fallback (no git entry) fires on the same synthetic tree shape.
        let dir2 = synthetic_fixture("8614-crlf-fallback", b"x\r\ny\r\n", true)?;
        let fallback = crlf_guard_violations_with(&dir2, &BTreeMap::new(), &BTreeSet::new());
        let _ = std::fs::remove_dir_all(&dir2);
        assert!(!fallback.is_empty(), "CRLF fallback must fire");
        Ok(())
    }

    #[test]
    fn crlf_guard_does_not_fire_on_lf_binary_or_non_identity_fixtures() -> Result<(), String> {
        // LF negative control.
        let dir = synthetic_fixture("8614-lf", b"fn a() {}\n", true)?;
        let key = normalize_path(&dir.join("input/src/lib.rs"));
        let mut eol = BTreeMap::new();
        eol.insert(
            key.clone(),
            GitEolInfo {
                index: "lf".into(),
                worktree: "lf".into(),
                attrs: "text=auto eol=lf".into(),
            },
        );
        assert!(crlf_guard_violations_with(&dir, &eol, &BTreeSet::new()).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        // -text binary fixture with CRLF bytes never fires.
        let dir = synthetic_fixture("8614-binary", b"a\r\nb\r\n", true)?;
        let key = normalize_path(&dir.join("input/src/lib.rs"));
        let mut eol = BTreeMap::new();
        eol.insert(
            key,
            GitEolInfo {
                index: "lf".into(),
                worktree: "crlf".into(),
                attrs: "-text".into(),
            },
        );
        assert!(crlf_guard_violations_with(&dir, &eol, &BTreeSet::new()).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        // No input_identity in expected output: guard is out of scope.
        let dir = synthetic_fixture("8614-no-identity", b"a\r\n", false)?;
        assert!(crlf_guard_violations_with(&dir, &BTreeMap::new(), &BTreeSet::new()).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn parse_git_ls_files_eol_reads_index_worktree_and_attrs() {
        let map = parse_git_ls_files_eol(b"i/lf    w/crlf  attr/text=auto eol=lf \tfixtures/x/input/src/lib.rs\0i/-text w/-text attr/-text\tassets/a.png\0");
        assert_eq!(map["fixtures/x/input/src/lib.rs"].worktree, "crlf");
        assert!(git_eol_info_fires(&map["fixtures/x/input/src/lib.rs"]));
        assert!(!git_eol_info_fires(&map["assets/a.png"]));
    }

    #[test]
    fn spec_line_placeholder_is_not_a_real_spec_id() {
        // #7399: the scaffold placeholder must not satisfy the fixture gate.
        assert!(!spec_line_cites_real_id("Spec: RIPR-SPEC-NNNN"));
        assert!(!spec_line_cites_real_id("Spec: RIPR-SPEC-12"));
        assert!(!spec_line_cites_real_id("Spec: RIPR-SPEC-00011"));
        assert!(!spec_line_cites_real_id("Spec: RIPR-SPEC-0001x"));
        assert!(spec_line_cites_real_id("Spec: RIPR-SPEC-0001"));
        assert!(spec_line_cites_real_id(
            "Spec: RIPR-SPEC-0062, RIPR-SPEC-0063"
        ));
    }

    #[test]
    fn scaffolded_fixture_with_placeholder_spec_fails_the_contract() -> Result<(), String> {
        let dir = unique_temp_path("7399-spec-placeholder");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("expected")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("diff.patch"), "").map_err(|e| e.to_string())?;
        std::fs::write(dir.join("expected/check.json"), "{}").map_err(|e| e.to_string())?;
        let body = |id: &str| {
            format!("# Fixture\n\nSpec: {id}\n\n## Given\n\n## When\n\n## Then\n\n## Must Not\n")
        };
        std::fs::write(dir.join("SPEC.md"), body("RIPR-SPEC-NNNN")).map_err(|e| e.to_string())?;
        let placeholder = fixture_contract_violations(&dir)?;
        std::fs::write(dir.join("SPEC.md"), body("RIPR-SPEC-0001")).map_err(|e| e.to_string())?;
        let real = fixture_contract_violations(&dir)?;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            placeholder
                .iter()
                .any(|v| v.contains("cites no real spec id")),
            "{placeholder:#?}"
        );
        assert!(real.is_empty(), "{real:#?}");
        Ok(())
    }

    #[test]
    fn golden_drift_reports_record_the_producing_build() -> Result<(), String> {
        // #7401: neither the drift report nor the console card recorded which
        // binary build produced the actual side; both now carry binary,
        // version, and revision in the header and in each JSON entry.
        let provenance = GoldenDriftProvenance {
            binary: "/work/target/debug/ripr".to_string(),
            ripr_version: "ripr 0.11.0 (abc123)".to_string(),
            revision: "abc123".to_string(),
        };
        let entries = vec![GoldenDriftEntry {
            fixture: "python_field_assignment_shape".to_string(),
            surface: "check.json".to_string(),
            expected: "fixtures/x/expected/check.json".to_string(),
            actual: "target/x/actual/check.json".to_string(),
            blessing_reason_required: true,
            blessing_reason_present: false,
            semantics: GoldenDriftSemantics::default(),
        }];
        let markdown = golden_drift_markdown(&entries, &[], &provenance);
        assert!(markdown.contains("## Provenance"), "{markdown}");
        assert!(
            markdown.contains("- binary: `/work/target/debug/ripr`"),
            "{markdown}"
        );
        assert!(
            markdown.contains("- ripr version: `ripr 0.11.0 (abc123)`"),
            "{markdown}"
        );
        assert!(markdown.contains("- revision: `abc123`"), "{markdown}");

        let json = golden_drift_json(&entries, &[], &provenance);
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| format!("golden drift JSON must parse: {error}"))?;
        assert_eq!(value["provenance"]["binary"], "/work/target/debug/ripr");
        assert_eq!(value["provenance"]["ripr_version"], "ripr 0.11.0 (abc123)");
        assert_eq!(value["provenance"]["revision"], "abc123");
        assert_eq!(
            value["entries"][0]["provenance"]["binary"],
            "/work/target/debug/ripr"
        );
        assert_eq!(
            value["entries"][0]["provenance"]["ripr_version"],
            "ripr 0.11.0 (abc123)"
        );
        assert_eq!(value["entries"][0]["provenance"]["revision"], "abc123");
        Ok(())
    }

    #[test]
    fn cwd_projection_unquotes_a_spaced_checkout_and_keeps_needed_quotes() {
        let prefix = "/tmp/my checkout/";
        // A bare tail renders quoted only because of the prefix: golden form.
        assert_eq!(
            project_cwd_prefix(
                "ripr explain --root '/tmp/my checkout/fixtures/x/input' --diff d",
                prefix
            ),
            "ripr explain --root <cwd>/fixtures/x/input --diff d"
        );
        // A tail that itself needs quoting keeps the quotes.
        assert_eq!(
            project_cwd_prefix("--root '/tmp/my checkout/repo root'", prefix),
            "--root '<cwd>/repo root'"
        );
        // A quote-free checkout projects the bare prefix.
        assert_eq!(
            project_cwd_prefix("--root /w/fixtures/x/input", "/w/"),
            "--root <cwd>/fixtures/x/input"
        );
        // #6762: a tail carrying an escaped quote is one token, not two.
        assert_eq!(
            project_cwd_prefix(r"--root '/tmp/my checkout/it'\''s/input' --diff d", prefix),
            r"--root '<cwd>/it'\''s/input' --diff d"
        );
        // So is a checkout whose own path carries the quote.
        assert_eq!(
            project_cwd_prefix(
                r"--root '/tmp/dev'\''s repo/fixtures/x/input' --diff d",
                "/tmp/dev's repo/"
            ),
            "--root <cwd>/fixtures/x/input --diff d"
        );
    }

    #[test]
    fn human_normalizer_never_rewrites_source_text_backslashes() {
        let human = [
            "root: \\u{7b}",
            "  root: \\u{7b}",
            "owner: \\u{7d}",
            "owner: \\n",
            "WARNING .\\src\\lib.rs:5",
            "  - src\\foo.rs:3 (weak)",
            "  before: write!(f, \"\\u{7b}\\u{7d}\", self.0)",
            "  - source assertion argument value \"read \\\"error\\\"\" at line 18",
            "  before: write!(f, r#\"prefix src\\foo.rs suffix\"#)",
            "ripr does not infer `\\u{7b}` here",
        ]
        .join("\n");
        assert_eq!(normalize_fixture_human_output(&human), format!("{human}\n"));
    }

    #[test]
    fn human_normalizer_projects_cwd_and_ends_with_one_newline() {
        let out = normalize_fixture_human_output("root: \\u{7b}\r\n\n");
        assert_eq!(out, "root: \\u{7b}\n");
    }

    /// Unique per process and per call. Fixed names under the shared temp
    /// directory collide when two `cargo test` processes run at once — two
    /// worktrees on one machine is the ordinary case here — and one process
    /// then deletes the path the other just created, failing the gate for an
    /// infrastructure reason. `xtask/src/run.rs` already builds temp paths this
    /// way.
    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ripr-xtask-3054-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    use super::*;

    #[test]
    fn normalize_json_handles_double_backslash() {
        let input = r#"{"path":"crates\\ripr\\src\\lib.rs"}"#;
        let out = normalize_fixture_json_output_for_host(input, true);
        assert!(
            !out.contains('\\'),
            "double backslash must be normalized: {out}"
        );
    }

    #[test]
    fn normalize_json_keeps_source_text_backslashes_off_windows() -> Result<(), String> {
        // #11119: a JSON `\\` is a literal backslash of asserted source text.
        // Source `"read \"error\"", "\u{7b}", "a\nb"` renders as the JSON below.
        let input =
            r#"{"oracle":"assert_eq!(x, \"read \\\"error\\\"\", \"\\u{7b}\", \"a\\nb\");"}"#;
        assert_eq!(normalize_fixture_json_output_for_host(input, false), input);
        let parsed: serde_json::Value =
            serde_json::from_str(input).map_err(|error| error.to_string())?;
        assert_eq!(
            parsed["oracle"],
            "assert_eq!(x, \"read \\\"error\\\"\", \"\\u{7b}\", \"a\\nb\");"
        );
        Ok(())
    }

    #[test]
    fn normalize_json_windows_keeps_quote_and_unicode_escapes_but_folds_paths() {
        let source =
            r#"{"oracle":"\"read \\\"error\\\"\" \\u{7b}","file":"crates\\ripr\\src\\lib.rs"}"#;
        let out = normalize_fixture_json_output_for_host(source, true);
        assert_eq!(
            out,
            r#"{"oracle":"\"read \\\"error\\\"\" \\u{7b}","file":"crates/ripr/src/lib.rs"}"#
        );
    }

    #[test]
    fn normalize_json_windows_folds_parenthesized_program_files_path() {
        let source = r#"{"root":"vendor (x86)\\ripr\\src"}"#;
        let out = normalize_fixture_json_output_for_host(source, true);
        assert_eq!(out, r#"{"root":"vendor (x86)/ripr/src"}"#);
    }

    #[test]
    fn normalize_json_windows_keeps_evidence_path_source_text() {
        // `evidence_path` contains "path" but is a text trail quoting asserted
        // source, so Windows must leave it identical to the other hosts while
        // the sibling `sample_paths` array still folds (#11119).
        let source =
            r#"{"evidence_path":["a \"r \\\"e\\\"\"","u \\u{7b}"],"sample_paths":["s\\a.rs"]}"#;
        let out = normalize_fixture_json_output_for_host(source, true);
        assert_eq!(
            out,
            r#"{"evidence_path":["a \"r \\\"e\\\"\"","u \\u{7b}"],"sample_paths":["s/a.rs"]}"#
        );
    }

    #[test]
    fn normalize_json_windows_tokenizer_handles_trailing_backslash_and_nesting() {
        // A string ending in an escaped backslash must close at its quote, and
        // nested values resolve against their own keys.
        let source = r#"{"root":"r\\s\\","n":{"note":"a\\b","dir":["x\\y"]},"k\"q":"v\\w"}"#;
        let out = normalize_fixture_json_output_for_host(source, true);
        assert_eq!(
            out,
            r#"{"root":"r/s/","n":{"note":"a\\b","dir":["x/y"]},"k\"q":"v\\w"}"#
        );
    }

    #[test]
    fn normalize_json_windows_keeps_newline_source_text_beside_path_fields() {
        // `a\nb` has path-character neighbours but lives under a source-text
        // key, so it must survive while the sibling path field folds (#11119).
        let source = r#"{"oracle":"a\\nb","files":["src\\a.rs","src\\b.rs"],"text":"x\\y"}"#;
        let out = normalize_fixture_json_output_for_host(source, true);
        assert_eq!(
            out,
            r#"{"oracle":"a\\nb","files":["src/a.rs","src/b.rs"],"text":"x\\y"}"#
        );
    }

    #[test]
    fn normalize_json_preserves_no_backslash_content() {
        let input = r#"{"path":"src/lib.rs","count":42}"#;
        let out = normalize_fixture_json_output(input);
        assert_eq!(out, input);
    }

    #[test]
    fn normalize_json_projects_checkout_cwd_prefix_in_config_path() -> Result<(), String> {
        // RIPR-SPEC-0255 (#7536): the check JSON names the loaded config by
        // its absolute stable spelling, so checked-in goldens must hold the
        // machine-independent `<cwd>/` placeholder, including the Windows
        // verbatim `//?/` form the lossless encoder preserves.
        let cwd = std::env::current_dir()
            .map_err(|error| format!("readable working directory: {error}"))?;
        let prefix = format!("{}/", normalize_path(&cwd));
        let plain = format!(
            "{{\"root\":\"fixtures/x/input\",\"config_path\":\"{prefix}fixtures/x/input/ripr.toml\"}}"
        );
        assert_eq!(
            normalize_fixture_json_output(&plain),
            "{\"root\":\"fixtures/x/input\",\"config_path\":\"<cwd>/fixtures/x/input/ripr.toml\"}"
        );
        let verbatim = format!("{{\"config_path\":\"//?/{prefix}fixtures/x/input/ripr.toml\"}}");
        assert_eq!(
            normalize_fixture_json_output(&verbatim),
            "{\"config_path\":\"<cwd>/fixtures/x/input/ripr.toml\"}"
        );
        Ok(())
    }

    #[test]
    fn normalize_json_strips_language_completeness_wherever_it_sits() {
        let member = "{\"mode\":\"diff\",\"languages\":[{\"language\":\"rust\"}]}";
        // After the findings array's own newline, ahead of later members.
        let input = format!(
            "{{\n  \"findings\": [\n  ]\n,\n  \"language_completeness\": {member},\n  \"source_subject\": {{\"a\":1}},\n  \"canonical_next_action\": {{\"x\":1}}\n}}\n"
        );
        assert_eq!(
            normalize_fixture_json_output(&input),
            "{\n  \"findings\": [\n  ],\n  \"source_subject\": {\"a\":1}\n}\n"
        );
        // Last member after a compact `]`.
        let compact =
            format!("{{\n  \"findings\": [\n  ],\n  \"language_completeness\": {member}}}\n");
        assert_eq!(
            normalize_fixture_json_output(&compact),
            "{\n  \"findings\": [\n  ]\n}\n"
        );
    }

    #[test]
    fn normalize_json_strips_canonical_next_action_without_touching_findings() -> Result<(), String>
    {
        let input = "{\n  \"findings\": [{\"id\":\"gap-1\"}],\n  \"source_subject\": {\"digest_algorithm\":\"sha256\"},\n  \"canonical_next_action\": {\"schema_version\":\"canonical_next_action.v1\",\"action_class\":\"inspect_details\"}\n}\n";
        let out = normalize_fixture_json_output(input);
        if out.contains("canonical_next_action") {
            return Err(format!(
                "fixture goldens must not absorb the check JSON action: {out}"
            ));
        }
        if !(out.contains("\"findings\"") && out.contains("source_subject")) {
            return Err(format!(
                "strip must keep the classification document: {out}"
            ));
        }
        let parsed: serde_json::Value = serde_json::from_str(&out)
            .map_err(|error| format!("stripped output must remain JSON: {error}"))?;
        if parsed.get("canonical_next_action").is_some() {
            return Err("parsed document still has canonical_next_action".to_string());
        }
        if parsed["findings"][0]["id"] != "gap-1" {
            return Err(format!("findings drifted: {parsed}"));
        }
        if parsed["source_subject"]["digest_algorithm"] != "sha256" {
            return Err(format!(
                "closing brace after the compact member must survive strip: {out}"
            ));
        }
        Ok(())
    }

    fn simulate_append_canonical_next_action(rendered: &str) -> String {
        let trimmed = rendered.trim_end();
        let Some(body) = trimmed.strip_suffix('}') else {
            return rendered.to_string();
        };
        let body = body.trim_end();
        let newline = if rendered.ends_with('\n') { "\n" } else { "" };
        format!(
            "{body},\n  \"canonical_next_action\": {{\"schema_version\":\"canonical_next_action.v1\"}}\n}}{newline}"
        )
    }

    #[test]
    fn compact_findings_closer_golden_matches_after_last_member_strip() -> Result<(), String> {
        let compact = "{\n  \"findings\": [\n    {\"id\":\"gap-1\"}\n  ]}\n";
        let pretty = "{\n  \"findings\": [\n    {\"id\":\"gap-1\"}\n  ]\n}\n";
        let appended = simulate_append_canonical_next_action(compact);
        let compact_out = normalize_check_json_golden_document(compact);
        let appended_out = normalize_check_json_golden_document(&appended);
        let pretty_out = normalize_check_json_golden_document(pretty);
        if compact_out != appended_out {
            return Err(format!(
                "compact findings closer golden must match reconstituted last-member strip\ncompact: {compact_out:?}\nappended: {appended_out:?}"
            ));
        }
        if compact_out != pretty_out {
            return Err(format!(
                "compact and pretty findings closers must share one normalized form\ncompact: {compact_out:?}\npretty: {pretty_out:?}"
            ));
        }
        if compact_out.contains("canonical_next_action") {
            return Err(format!("strip leaked the last member: {compact_out}"));
        }
        let parsed: serde_json::Value = serde_json::from_str(&compact_out)
            .map_err(|error| format!("normalized compact closer must stay JSON: {error}"))?;
        if parsed["findings"][0]["id"] != "gap-1" {
            return Err(format!("findings drifted: {parsed}"));
        }
        Ok(())
    }

    #[test]
    fn one_line_findings_document_keeps_its_compact_closer() {
        let input = r#"{"findings":[{"id":"gap-1"}]}"#;
        assert_eq!(normalize_check_json_golden_document(input), input);
    }

    #[test]
    fn compact_closer_normalization_still_sees_finding_id_drift() {
        let left = "{\n  \"findings\": [\n    {\"id\":\"gap-1\"}\n  ]}\n";
        let right = "{\n  \"findings\": [\n    {\"id\":\"gap-2\"}\n  ]}\n";
        assert_ne!(
            normalize_check_json_golden_document(left),
            normalize_check_json_golden_document(right)
        );
    }

    #[test]
    fn refusal_reason_fixed_wording_is_compared_and_only_moving_values_are_masked() {
        let wrap = |reason: &str| {
            format!("Next:\n  ripr explain x\n  no repair start offered: {reason}\nMore:\n")
        };
        // Fixed-wording arms are compared verbatim (#12003): rewording one,
        // even to another word for the same idea, is golden drift.
        let fixed = [
            "this finding has no gap ID, so there is no gap to start a repair from; follow the Next step above",
            "ripr found no seam in this finding's file to bind it to, so there is no seam to start a repair from; follow the Next step above",
            "several seams bind this finding and the check screen never picks one",
            "nothing to repair here; the related tests already notice this change (exposed)",
            "`ripr check` reads a gap or limit for this change, but words",
            "the root's config could not be read; fix the ripr.toml error at the root, then rerun `ripr check`",
            "ripr cannot build a repair packet for this finding yet because the static evidence is not complete enough to name a safe test target; follow the Next step above, then rerun `ripr check`",
            "the seam budget left the repair candidate set unreadable or truncated; rerun",
            "ripr cannot build a repair packet for this finding yet because it is still missing: a; b; follow the Next step above, then rerun `ripr check`",
            "a related test already reaches the changed boundary input, but words, yet. It is also still missing: q. Follow the Next step above, then rerun `ripr check`",
        ];
        for reason in fixed {
            assert_eq!(
                mask_repair_refusal_reason(&wrap(reason)),
                wrap(reason),
                "fixed refusal wording must stay compared: {reason}"
            );
            let reworded = reason.replacen(' ', "  ", 1);
            assert_ne!(
                mask_repair_refusal_reason(&wrap(reason)),
                mask_repair_refusal_reason(&wrap(&reworded)),
                "a reworded refusal must read as golden drift: {reason}"
            );
        }
        // Arms that embed a language, file or command keep a fixed opening and
        // mask only what follows it.
        for opening in [
            "the seam inventory behind `ripr repair`",
            "classifying the finding's seams failed",
        ] {
            assert_eq!(
                mask_repair_refusal_reason(&wrap(&format!("{opening} one"))),
                mask_repair_refusal_reason(&wrap(&format!("{opening} two")))
            );
        }
        // The preview-packet arm masks only the cited test location (#12086).
        let preview = |language: &str, cite: &str, tail: &str| {
            wrap(&format!(
                "ripr has no complete repair packet for this {language} finding (ripr cannot tell whether the test at `{cite}` covers this change: {tail}); add or strengthen the test by hand as the Next step above says, then rerun `ripr check`"
            ))
        };
        let base = preview(
            "TypeScript",
            "tests/a.test.ts:8",
            "its `.toBe(...)` compares against a variable",
        );
        // A moved location is not drift.
        assert_eq!(
            mask_repair_refusal_reason(&base),
            mask_repair_refusal_reason(&preview(
                "TypeScript",
                "tests/b.test.ts:12",
                "its `.toBe(...)` compares against a variable"
            ))
        );
        // A changed language, matcher or reason wording is drift.
        for changed in [
            preview(
                "JavaScript",
                "tests/a.test.ts:8",
                "its `.toBe(...)` compares against a variable",
            ),
            preview(
                "TypeScript",
                "tests/a.test.ts:8",
                "its `.toEqual(...)` compares against a variable",
            ),
            preview(
                "TypeScript",
                "tests/a.test.ts:8",
                "its `.toBe(...)` compares against a computed value",
            ),
        ] {
            assert_ne!(
                mask_repair_refusal_reason(&base),
                mask_repair_refusal_reason(&changed)
            );
        }
        // A reason with no cited location is compared verbatim.
        let no_cite = |name: &str| {
            wrap(&format!(
                "ripr has no complete repair packet for this TypeScript finding (related test `{name}` asserts a different kind of behavior); add or strengthen the test by hand"
            ))
        };
        assert_ne!(
            mask_repair_refusal_reason(&no_cite("one")),
            mask_repair_refusal_reason(&no_cite("two"))
        );
        // Negative: a variant change is drift even though both are refusals.
        assert_ne!(
            mask_repair_refusal_reason(&wrap(fixed[0])),
            mask_repair_refusal_reason(&wrap(fixed[1]))
        );
        let canonical = wrap(fixed[0]);
        // Negative: an unknown reason is compared verbatim.
        assert_ne!(
            mask_repair_refusal_reason(&wrap("something unmapped one")),
            mask_repair_refusal_reason(&wrap("something unmapped two"))
        );
        // Negative: a repair start that flips to a refusal is still drift.
        let offered = "Next:\n  ripr explain x\n  ripr repair abc --root r\nMore:\n";
        assert_ne!(
            mask_repair_refusal_reason(offered),
            mask_repair_refusal_reason(&canonical)
        );
        // Negative: a refusal that disappears is still drift.
        let absent = "Next:\n  ripr explain x\nMore:\n";
        assert_ne!(
            mask_repair_refusal_reason(absent),
            mask_repair_refusal_reason(&canonical)
        );
        // Only the marker line is masked; prose elsewhere stays compared, and
        // CRLF and a missing final newline are preserved.
        assert_ne!(
            mask_repair_refusal_reason("a\n  no repair start offered: x\n"),
            mask_repair_refusal_reason("b\n  no repair start offered: x\n")
        );
        assert_eq!(
            mask_repair_refusal_reason(
                "  no repair start offered: classifying the finding's seams failed, x\r\n"
            ),
            "  no repair start offered: classifying the finding's seams failed<reason>\r\n"
        );
        assert_eq!(
            mask_repair_refusal_reason(
                "  no repair start offered: classifying the finding's seams failed"
            ),
            "  no repair start offered: classifying the finding's seams failed<reason>"
        );
    }

    /// #12163: moving a cited TypeScript test shifts its line in every surface
    /// that names it. The real `ts_dynamic_oracle` golden, with each cited
    /// test line moved, must still compare equal; a changed blocker wording, a
    /// changed oracle or a moved *source* line must not.
    #[test]
    fn moved_typescript_test_line_is_not_drift_on_any_surface_but_other_changes_are()
    -> Result<(), String> {
        let fixture = "ts_dynamic_oracle";
        let root =
            crate::dogfood::repo_rooted_fixture_path(&format!("fixtures/{fixture}/expected"));
        let dir = unique_temp_path("12163-moved-test-line");
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let shift = |text: &str, from: &str, to: &str| text.replace(from, to);
        for surface in ["human.txt", "check.json"] {
            let golden_path = root.join(surface);
            let golden = std::fs::read_to_string(&golden_path).map_err(|err| err.to_string())?;
            let write = |name: &str, text: &str| -> Result<std::path::PathBuf, String> {
                let path = dir.join(format!("{surface}.{name}"));
                std::fs::write(&path, text).map_err(|err| err.to_string())?;
                Ok(path)
            };
            let compare = |actual: &Path| -> Result<bool, String> {
                Ok(compare_golden(
                    fixture,
                    if surface == "human.txt" {
                        "human.txt"
                    } else {
                        "check.json"
                    },
                    &golden_path,
                    actual,
                )?
                .matches)
            };
            // Fixture-construction guard: the golden really cites the test.
            assert!(
                golden.contains("tests/pricing.test.ts:5") || golden.contains("\"line\": 3"),
                "{surface}: fixture no longer cites the test line"
            );
            // Move every cited test line (citations and structured lines).
            let moved = shift(
                &golden,
                "tests/pricing.test.ts:5",
                "tests/pricing.test.ts:9",
            );
            let moved = shift(&moved, "tests/pricing.test.ts:3", "tests/pricing.test.ts:7");
            let moved = if surface == "check.json" {
                moved
                    .replace(
                        "\"file\": \"tests/pricing.test.ts\",\n          \"line\": 3",
                        "\"file\": \"tests/pricing.test.ts\",\n          \"line\": 7",
                    )
                    .replace(
                        "\"file\": \"tests/pricing.test.ts\",\n          \"line\": 3,",
                        "\"file\": \"tests/pricing.test.ts\",\n          \"line\": 7,",
                    )
            } else {
                moved
            };
            assert_ne!(moved, golden, "{surface}: the moved copy must differ");
            assert!(
                compare(&write("moved", &moved)?)?,
                "{surface}: moved test line is not drift"
            );
            // Negative controls: other changes remain drift.
            let reworded = shift(
                &moved,
                "compares against a variable",
                "compares against a computed value",
            );
            if reworded != moved {
                assert!(
                    !compare(&write("reworded", &reworded)?)?,
                    "{surface}: reworded blocker is drift"
                );
            }
            let other_test = shift(&moved, "tests/pricing.test.ts:9", "tests/other.test.ts:9");
            if other_test != moved {
                assert!(
                    !compare(&write("other_file", &other_test)?)?,
                    "{surface}: a different test file is drift"
                );
            }
            if surface == "check.json" {
                for broken in ["null", "\"7\"", "7x", "-7", "07", "7,,", ""] {
                    let invalid = moved.replace(
                        "\"file\": \"tests/pricing.test.ts\",\n          \"line\": 7",
                        &format!(
                            "\"file\": \"tests/pricing.test.ts\",\n          \"line\": {broken}"
                        ),
                    );
                    assert_ne!(
                        invalid, moved,
                        "the invalid-line control must edit the JSON"
                    );
                    assert!(
                        !compare(&write("invalid_line", &invalid)?)?,
                        "a test line of `{broken}` is drift, not a masked line"
                    );
                }
                // A `line` in a later object with no `file` of its own is not
                // governed by the earlier test-file object.
                let closed =
                    "{\n  \"file\": \"tests/a.test.ts\",\n  \"line\": 3\n},\n{\n  \"line\": ";
                assert_ne!(
                    mask_test_lines_in_check_json(&format!("{closed}3\n}}")),
                    mask_test_lines_in_check_json(&format!("{closed}4\n}}")),
                    "a bare line after a closed test-file object stays compared"
                );
                assert_eq!(
                    mask_test_lines_in_check_json(
                        "{\n  \"file\": \"tests/a.test.ts\",\n  \"line\": 3\n}"
                    ),
                    mask_test_lines_in_check_json(
                        "{\n  \"file\": \"tests/a.test.ts\",\n  \"line\": 4\n}"
                    ),
                    "a test-file line inside its own object is masked"
                );
                let source_moved = shift(&moved, "\"line\": 2", "\"line\": 3");
                assert_ne!(
                    source_moved, moved,
                    "check.json cites a source line to move"
                );
                assert!(
                    !compare(&write("source_moved", &source_moved)?)?,
                    "a moved source line is drift"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn compare_golden_masks_human_refusal_prose_but_not_variant_or_check_json() -> Result<(), String>
    {
        let dir = unique_temp_path("10702-compare-golden");
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let write = |name: &str, text: &str| -> Result<std::path::PathBuf, String> {
            let path = dir.join(name);
            std::fs::write(&path, text).map_err(|err| err.to_string())?;
            Ok(path)
        };
        let line = |reason: &str| format!("Next:\n  no repair start offered: {reason}\n");
        let expected = write(
            "expected.txt",
            &line(
                "ripr cannot build a repair packet for this finding yet because it is still missing: a; b; follow the Next step above, then rerun `ripr check`",
            ),
        )?;
        let reworded = write(
            "reworded.txt",
            &line(
                "ripr cannot build a repair packet for this finding yet because it is still missing: a; b; follow the Next step above, then try again",
            ),
        )?;
        let other_list = write(
            "other_list.txt",
            &line(
                "ripr cannot build a repair packet for this finding yet because it is still missing: a; c; follow the Next step above",
            ),
        )?;
        let other_variant = write(
            "other_variant.txt",
            &line("this finding has no gap ID, so there is no seam"),
        )?;
        let matches = |actual: &Path, surface: &'static str| -> Result<bool, String> {
            Ok(compare_golden("unlisted_fixture", surface, &expected, actual)?.matches)
        };
        assert!(
            !matches(&reworded, "human.txt")?,
            "fixed advice wording is pinned (#12003)"
        );
        let inventory_a = write(
            "inventory_a.txt",
            &line("the seam inventory behind `ripr repair` has no python file a"),
        )?;
        let inventory_b = write(
            "inventory_b.txt",
            &line("the seam inventory behind `ripr repair` has no python file b"),
        )?;
        assert!(
            compare_golden("unlisted_fixture", "human.txt", &inventory_a, &inventory_b)?.matches,
            "an embedded language/file value is masked"
        );
        assert!(
            !matches(&other_list, "human.txt")?,
            "missing list is pinned"
        );
        assert!(!matches(&other_variant, "human.txt")?, "variant is pinned");
        // check.json is never refusal-masked: the same text diverges there.
        assert!(!matches(&reworded, "check.json")?, "check.json is unmasked");
        assert!(matches(&expected, "check.json")?);

        // Transitional migrations (#10702): only an approved fixture/surface
        // with an approved old -> new opening passes.
        let exposed_header = "Static exposure: exposed (severity: info)\n";
        let write_exposed = |name: &str, reason: &str| -> Result<std::path::PathBuf, String> {
            write(name, &format!("{exposed_header}{}", line(reason)))
        };
        let exposed_line =
            "nothing to repair here; the related tests already notice this change (exposed)";
        let untied_line = "ripr found 4 spots in this finding's file, but could not tie one to this finding's changed line, so y";
        let one_seam_line = "ripr found 1 spot in this finding's file, but could not tie one to this finding's changed line, so z";
        let old_no_seam = write_exposed(
            "old_no_seam.txt",
            "ripr found no seam in this finding's file to bind it to, so x",
        )?;
        let exposed = write_exposed("exposed.txt", exposed_line)?;
        let untied = write_exposed("untied.txt", untied_line)?;
        let one_seam = write_exposed("one_seam.txt", one_seam_line)?;
        let compare = |fixture: &str,
                       exp: &Path,
                       act: &Path,
                       surface: &'static str|
         -> Result<bool, String> {
            Ok(compare_golden(fixture, surface, exp, act)?.matches)
        };
        // Real approved identities, one per migration.
        let no_seam_exposed = "asserting_same_file_test_positive_control";
        let no_seam_untied = "assertion_shaped_control_production_caller";
        let missing_exposed = "assertion_shaped_control_production_owner";
        let inventory_exposed = "currentness_matrix_python_deleted_statement";
        // Retired by the TypeScript/JavaScript re-bless (#10700, #10885): the
        // no-gap-ID -> exposed entries were deleted, so it no longer aliases.
        let retired_no_gap = "currentness_matrix_typescript_deleted_statement";
        assert!(compare(
            no_seam_exposed,
            &old_no_seam,
            &exposed,
            "human.txt"
        )?);
        assert!(compare(no_seam_untied, &old_no_seam, &untied, "human.txt")?);
        assert!(
            compare(no_seam_untied, &old_no_seam, &one_seam, "human.txt")?,
            "singular seam"
        );
        let inventory = write_exposed(
            "inventory.txt",
            "the seam inventory behind `ripr repair` and `ripr agent repair --phase before` could not be built",
        )?;
        assert!(
            compare(inventory_exposed, &inventory, &exposed, "human.txt")?,
            "inventory -> exposed"
        );
        let no_gap = write_exposed(
            "no_gap.txt",
            "this finding has no gap ID, so there is no seam",
        )?;
        assert!(
            !compare(retired_no_gap, &no_gap, &exposed, "human.txt")?,
            "retired no gap ID -> exposed migration is drift again"
        );
        let missing = write_exposed(
            "missing.txt",
            "ripr cannot build a repair packet for this finding yet because it is still missing: a; b; follow the Next step above",
        )?;
        assert!(
            compare(missing_exposed, &missing, &exposed, "human.txt")?,
            "still missing -> exposed"
        );
        // Negatives: an unapproved fixture, the wrong surface, check.json, an
        // approved fixture with a pair it is not approved for, and pairs that
        // are not aliases at all.
        assert!(
            !compare("unlisted_fixture", &old_no_seam, &exposed, "human.txt")?,
            "unapproved fixture"
        );
        assert!(
            !compare(no_seam_exposed, &old_no_seam, &exposed, "human-full.txt")?,
            "unapproved surface"
        );
        assert!(
            !compare(no_seam_exposed, &old_no_seam, &exposed, "check.json")?,
            "check.json"
        );
        assert!(
            !compare(no_seam_exposed, &old_no_seam, &untied, "human.txt")?,
            "approved fixture, other pair"
        );
        assert!(
            !compare(inventory_exposed, &inventory, &untied, "human.txt")?,
            "inventory -> untied"
        );
        assert!(
            !compare(no_seam_untied, &untied, &exposed, "human.txt")?,
            "new -> new"
        );
        assert!(
            !compare(no_seam_exposed, &exposed, &untied, "human.txt")?,
            "exposed is not an alias source"
        );
        assert!(!compare(
            no_seam_exposed,
            &old_no_seam,
            &other_variant,
            "human.txt"
        )?);
        // The exposed target also needs the expected golden to say exposed.
        let old_no_seam_not_exposed = write(
            "old_no_seam_not_exposed.txt",
            &line("ripr found no seam in this finding's file to bind it to, so x"),
        )?;
        let exposed_no_header = write("exposed_no_header.txt", &line(exposed_line))?;
        assert!(
            !compare(
                no_seam_exposed,
                &old_no_seam_not_exposed,
                &exposed_no_header,
                "human.txt"
            )?,
            "expected must say exposed"
        );
        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(())
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// #3054 root cause: the previous guard was `if !binary.exists()`, so a
    /// `target/debug/ripr` left by an earlier build was reused and the golden
    /// gates validated previous code. The binary existing must not skip the
    /// build.
    #[test]
    fn ripr_binary_build_runs_even_when_the_binary_is_already_present() -> Result<(), String> {
        let dir = unique_temp_path("present");
        fs::create_dir_all(&dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
        let binary = dir.join(format!("ripr{}", std::env::consts::EXE_SUFFIX));
        fs::write(&binary, b"stale").map_err(|err| format!("write {}: {err}", binary.display()))?;
        assert!(binary.exists(), "probe must start from an existing binary");

        let builds = AtomicUsize::new(0);
        let cell = OnceLock::new();
        let resolved = ripr_fixture_binary_built_by(binary.clone(), &cell, &|| {
            builds.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })?;

        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "an existing binary must still be rebuilt"
        );
        assert!(
            resolved.ends_with(&format!("ripr{}", std::env::consts::EXE_SUFFIX)),
            "resolved path should name the binary: {resolved}"
        );
        fs::remove_dir_all(&dir).map_err(|err| format!("cleanup {}: {err}", dir.display()))
    }

    /// The build is unconditional per process, not per spawn: `collect_golden_runs`
    /// reaches the binary once per fixture through rayon, and one `cargo build`
    /// per fixture would serialise the corpus behind the package lock.
    #[test]
    fn ripr_binary_build_runs_once_per_process() -> Result<(), String> {
        let binary = unique_temp_path("once");
        let builds = AtomicUsize::new(0);
        let cell = OnceLock::new();
        let build = || {
            builds.fetch_add(1, Ordering::SeqCst);
            Ok(())
        };

        ripr_fixture_binary_built_by(binary.clone(), &cell, &build)?;
        ripr_fixture_binary_built_by(binary, &cell, &build)?;

        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "the build must be memoised across spawns"
        );
        Ok(())
    }

    /// A failed build must not degrade into "here is a path to whatever is on
    /// disk" — that is exactly the stale-artifact validation #3054 removes.
    #[test]
    fn ripr_binary_build_failure_is_reported_instead_of_a_path() -> Result<(), String> {
        let binary = unique_temp_path("failure");
        let cell = OnceLock::new();

        let first = ripr_fixture_binary_built_by(binary.clone(), &cell, &|| {
            Err("cargo build -p ripr failed with exit status: 101".to_string())
        });
        // A remembered failure stays a failure: a later spawn must not read a
        // memoised `Ok` out of a build that never succeeded.
        let second = ripr_fixture_binary_built_by(binary, &cell, &|| Ok(()));

        for result in [first, second] {
            let Err(err) = result else {
                return Err("a failed build must not resolve a binary path".to_string());
            };
            assert!(
                err.contains("cargo build -p ripr failed"),
                "the build failure must be reported verbatim: {err}"
            );
        }
        Ok(())
    }

    /// #3054: the cache a fixture run reads is resolved from its `--root`, which
    /// is `<fixture>/input`. A cache path that sits inside the fixture — or at
    /// the repository root, which the fixture run never consults — is the wrong
    /// target.
    #[test]
    fn fixture_cache_dir_is_absolute_and_outside_the_fixture_workspace() -> Result<(), String> {
        let dir = fixture_cache_dir("propagate_swallowed_ok")?;

        assert!(dir.is_absolute(), "cache dir must be absolute: {dir:?}");
        let normalized = normalize_path(&dir);
        assert!(
            normalized.ends_with("target/ripr/fixture-cache/propagate_swallowed_ok"),
            "cache dir must be per-fixture under the build tree: {normalized}"
        );
        assert!(
            !normalized.contains("fixtures/propagate_swallowed_ok"),
            "cache dir must not live inside the tracked fixture corpus: {normalized}"
        );
        assert!(
            !normalized.ends_with("target/ripr/cache"),
            "the repository cache root is not the cache a fixture run reads: {normalized}"
        );

        let other = fixture_cache_dir("propagate_value_returned")?;
        assert_ne!(
            dir, other,
            "each fixture must own its cache so one fixture cannot serve another's facts"
        );
        Ok(())
    }

    #[test]
    fn clear_fixture_cache_removes_entries_and_tolerates_absence() -> Result<(), String> {
        let dir = unique_temp_path("clear");
        let entry = dir.join("repo-file-facts").join("0.2").join("stale.json");
        let parent = entry
            .parent()
            .ok_or_else(|| "seeded cache entry should have a parent".to_string())?;
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
        fs::write(&entry, b"{}").map_err(|err| format!("write {}: {err}", entry.display()))?;

        clear_fixture_cache(&dir)?;
        assert!(!entry.exists(), "a stale cache entry must not survive");
        assert!(!dir.exists(), "the cache directory must be removed");

        clear_fixture_cache(&dir)?;
        Ok(())
    }

    /// A removal that failed must not be indistinguishable from one that
    /// succeeded: swallowing the error leaves the gate claiming a fresh cache it
    /// never got.
    #[test]
    fn clear_fixture_cache_reports_a_removal_failure() -> Result<(), String> {
        let path = unique_temp_path("not-a-dir");
        fs::write(&path, b"occupied").map_err(|err| format!("write {}: {err}", path.display()))?;

        let Err(err) = clear_fixture_cache(&path) else {
            let _ = fs::remove_file(&path);
            return Err("clearing a non-directory must be reported, not swallowed".to_string());
        };
        assert!(
            err.contains("failed to clear fixture fact cache"),
            "the failure must name what could not be cleared: {err}"
        );
        fs::remove_file(&path).map_err(|err| format!("cleanup {}: {err}", path.display()))
    }

    /// The pin only works if it names the variable `ripr` resolves its cache base
    /// from (`CACHE_DIR_ENV` in `crates/ripr/src/analysis/seam_cache.rs`).
    #[test]
    fn fixture_cache_pin_names_the_variable_ripr_resolves() {
        assert_eq!(FIXTURE_CACHE_DIR_ENV, "RIPR_CACHE_DIR");
    }

    /// #10702: each listed no-seam surface approves exactly one new opening.
    #[test]
    fn transitional_no_seam_surface_approves_only_its_own_new_opening() -> Result<(), String> {
        let line = |body: &str| format!("Start here:\n  {REPAIR_REFUSAL_MARKER}{body}\nEnd");
        let old = line(&format!("{} so there is no seam", transitional::NO_SEAM));
        let untied = line(&format!("{}<reason>", transitional::SEAMS_UNTIED));
        let gripped = line(&format!("{} but", transitional::SEAM_GRIPPED));
        let untied_list = transitional::NO_SEAM_UNTIED_SURFACES;
        let gripped_list = transitional::NO_SEAM_GRIPPED_SURFACES;
        if untied_list.len() != 44 || gripped_list.len() != 92 {
            return Err(format!(
                "expected 44 untied and 92 gripped surfaces, got {} and {}",
                untied_list.len(),
                gripped_list.len()
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in untied_list.iter().chain(gripped_list) {
            if !seen.insert(*key) {
                return Err(format!("`{key}` is listed more than once or in both lists"));
            }
        }
        let split = |key: &str| -> Result<(String, String), String> {
            key.split_once('/')
                .map(|(f, sfc)| (f.to_string(), sfc.to_string()))
                .ok_or_else(|| format!("malformed surface key `{key}`"))
        };
        for (list, own, other) in [
            (untied_list, &untied, &gripped),
            (gripped_list, &gripped, &untied),
        ] {
            for key in list {
                let (fixture, surface) = split(key)?;
                if !transitional_refusal_aliases_apply(&fixture, &surface, &old, own) {
                    return Err(format!("`{key}` must approve its own new opening"));
                }
                if transitional_refusal_aliases_apply(&fixture, &surface, &old, other) {
                    return Err(format!("`{key}` must reject the other new opening"));
                }
            }
        }
        let reindented = gripped.replace("  no repair start", "    no repair start");
        if reindented == gripped
            || transitional_refusal_aliases_apply("match_arm_blind", "human.txt", &old, &reindented)
        {
            return Err("an indentation change must not pass the alias".to_string());
        }
        let tabbed = gripped.replace("  no repair start", "\t no repair start");
        if tabbed == gripped
            || tabbed.len() != gripped.len()
            || transitional_refusal_aliases_apply("match_arm_blind", "human.txt", &old, &tabbed)
        {
            return Err("an equal-length whitespace substitution must not pass".to_string());
        }
        if transitional_refusal_aliases_apply("match_arm_blind", "check.json", &old, &gripped) {
            return Err("an unlisted surface stays strict".to_string());
        }
        Ok(())
    }

    #[test]
    fn automation_guide_bless_example_passes_the_installed_reason_contract() -> Result<(), String> {
        let _cwd_guard = crate::acquire_test_cwd_read_guard();
        let doc = include_str!("../../../docs/PR_AUTOMATION.md");
        let (_, section) = doc
            .split_once("### Mutating Shape Commands\n")
            .ok_or("automation guide lost the mutating command examples")?;
        let section = section.split("\n### ").next().ok_or("missing section")?;
        let examples = section
            .lines()
            .filter(|line| line.starts_with("cargo xtask goldens bless "))
            .collect::<Vec<_>>();
        if examples.len() != 1 {
            return Err(format!(
                "expected one golden-bless example, got {examples:?}"
            ));
        }
        let reason = bless_reason_quoted_in_step4(examples[0])?;
        if reason.contains("...") {
            return Err("guide still documents an ellipsis bless reason".to_string());
        }
        if parse_reason(&["--reason".to_string(), reason.clone()]).is_ok() {
            return Err(
                "unfilled template reason must fail parse_reason before substitution".to_string(),
            );
        }
        let resolved = reason.replace("RIPR-SPEC-NNNN", "RIPR-SPEC-0001").replace(
            "<reviewed change>",
            "reviewed expected output for the fixture",
        );
        let parsed = parse_reason(&["--reason".to_string(), resolved.clone()])?;
        validate_bless_reason(&parsed)?;

        let absent = "__automation_guide_bless_smoke_absent__";
        if fixture_dir_for_name(absent)?.exists() {
            return Err("smoke requires its fixture name to be absent".to_string());
        }
        let invoke = |reason: &str| {
            goldens_impl(&[
                "bless".to_string(),
                absent.to_string(),
                "--reason".to_string(),
                reason.to_string(),
            ])
        };
        let error = invoke(&resolved)
            .err()
            .ok_or("absent fixture was accepted after valid reason")?;
        if !error.starts_with("fixture does not exist:") {
            return Err(format!(
                "documented reason failed before fixture lookup: {error}"
            ));
        }
        for invalid in ["...", "RIPR-SPEC-9999: unknown spec control"] {
            let error = invoke(invalid).err().ok_or("invalid reason was accepted")?;
            if !error.contains("spec id") {
                return Err(format!("invalid reason reached fixture lookup: {error}"));
            }
        }
        Ok(())
    }

    #[test]
    fn fixtures_new_printed_bless_reason_passes_the_reason_validator() -> Result<(), String> {
        // #7398: the scaffold used to print `--reason "initial scaffold"`, which
        // `validate_bless_reason` / `parse_reason` reject because they require a
        // live spec id. The printed step-4 reason must be the same string the
        // validator accepts.
        let step4 = fixtures_new_step4("example_fixture");
        let reason = bless_reason_quoted_in_step4(&step4)?;
        if reason == "initial scaffold" {
            return Err("step 4 still suggests a reason with no spec id".to_string());
        }
        if validate_bless_reason("initial scaffold").is_ok() {
            return Err(
                "control: bare \"initial scaffold\" must still fail the validator".to_string(),
            );
        }
        if !step4.contains("cargo xtask specs next") {
            return Err(format!(
                "step 4 must point at `cargo xtask specs next` so the example spec can be replaced:\n{step4}"
            ));
        }
        if !step4.contains("cargo xtask goldens bless example_fixture --reason") {
            return Err(format!(
                "step 4 must cite the fixture name in the bless command:\n{step4}"
            ));
        }
        validate_bless_reason(&reason).map_err(|error| {
            format!("printed step-4 reason {reason:?} must pass validate_bless_reason: {error}")
        })
    }

    fn bless_reason_quoted_in_step4(step4: &str) -> Result<String, String> {
        const NEEDLE: &str = "--reason \"";
        let Some(start) = step4.find(NEEDLE) else {
            return Err(format!("step 4 does not quote a --reason: {step4}"));
        };
        let rest = step4
            .get(start.saturating_add(NEEDLE.len())..)
            .ok_or_else(|| format!("step 4 --reason needle split a character: {step4}"))?;
        let Some(end) = rest.find('"') else {
            return Err(format!("step 4 --reason is not closed: {step4}"));
        };
        rest.get(..end)
            .map(str::to_string)
            .ok_or_else(|| format!("step 4 --reason quote split a character: {step4}"))
    }
}

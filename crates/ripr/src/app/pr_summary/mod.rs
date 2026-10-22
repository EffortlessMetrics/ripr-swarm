//! `ripr pr-summary` — binary-first PR readiness summary (Campaign 31 item 8).
//!
//! Ports the `cargo xtask ripr-pr-summary` report into the `ripr` binary so
//! downstream consumers (e.g. perl-lsp-swarm) can generate their PR readiness
//! packet without compiling their own `xtask`. The xtask wrapper remains as a
//! compatibility shim until downstream consumers migrate.
//!
//! This command composes existing RIPR artifacts (start-here, gap-decision-
//! ledger, repo-exposure, diff-report) into a PR evidence summary. It does NOT
//! run analysis, invoke Cargo, or change gate semantics.

mod io;
mod json;
mod model;
mod render;
mod util;

use io::load_json;
pub use json::{build_pr_evidence_summary, render_pr_evidence_summary_json};
pub use model::PrEvidenceSummaryJson;
pub use render::render_evidence_summary_md;
use render::{SummaryRenderInput, render_pr_evidence_summary};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::{expect_value, unknown_argument};

const PR_EVIDENCE_JSON: &str = "target/ripr/pr/repo-exposure.json";
const PR_EVIDENCE_MD: &str = "target/ripr/pr/repo-exposure.md";
const REVIEW_COMMENTS_JSON: &str = "target/ripr/review/comments.json";
const REVIEW_COMMENTS_MD: &str = "target/ripr/review/comments.md";
const START_HERE_JSON: &str = "target/ripr/reports/start-here.json";
const START_HERE_MD: &str = "target/ripr/reports/start-here.md";
const PR_SUMMARY_MD: &str = "target/ripr/pr/summary.md";
const PR_EVIDENCE_SUMMARY_JSON: &str = "target/ripr/reports/pr-evidence-summary.json";
const PR_EVIDENCE_SUMMARY_MD: &str = "target/ripr/reports/pr-evidence-summary.md";
const GAP_DECISION_LEDGER_JSON: &str = "target/ripr/reports/gap-decision-ledger.json";
const REPO_EXPOSURE_JSON: &str = "target/ripr/reports/repo-exposure.json";
const DIFF_REPORT_JSON: &str = "target/ripr/reports/diff-report.json";
const ATTEMPT_LEDGER_JSON: &str = "target/ripr/reports/swarm-attempt-ledger.json";

#[derive(Clone, Debug, Eq, PartialEq)]
struct SummaryOptions {
    root: PathBuf,
    check: bool,
    baseline: Option<String>,
}

/// Entry point for `ripr pr-summary`. Composes existing artifacts into a PR
/// readiness summary. Writes three outputs:
/// - `target/ripr/pr/summary.md` (legacy PR evidence summary)
/// - `target/ripr/reports/pr-evidence-summary.json` (v1 JSON)
/// - `target/ripr/reports/pr-evidence-summary.md` (v1 Markdown panel)
pub(crate) fn run_pr_summary(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    let options = parse_options(args)?;
    let repo = repo_root(&options.root)?;
    // The selected root owns every input and output. A missing or file-typed
    // root must refuse before any write: `create_dir_all` below would
    // otherwise conjure the missing tree and report a summary of nothing.
    if !repo.is_dir() {
        return Err(format!(
            "pr-summary root {} is not a directory; pass `--root` naming an existing repository directory",
            options.root.display()
        ));
    }
    let summary = summary_text(&repo);
    let path = repo.join(PR_SUMMARY_MD);
    if options.check {
        check_summary(&path, &summary)?;
    } else {
        write_summary(&path, &summary)?;
    }
    write_evidence_summary_pair(&repo, &options)?;
    Ok(())
}

fn write_evidence_summary_pair(repo: &Path, options: &SummaryOptions) -> Result<(), String> {
    let start_here = load_json(repo, START_HERE_JSON);
    let gap_ledger = load_json(repo, GAP_DECISION_LEDGER_JSON);
    let repo_exposure = load_json(repo, REPO_EXPOSURE_JSON);
    let diff_report = load_json(repo, DIFF_REPORT_JSON);
    let attempt_ledger = load_json(repo, ATTEMPT_LEDGER_JSON);
    let baseline_loaded;
    let baseline_value = if let Some(path) = options.baseline.as_deref() {
        baseline_loaded = load_json(repo, path);
        baseline_loaded.value.as_ref()
    } else {
        None
    };

    let summary_struct = json::build_pr_evidence_summary_for_root(
        repo,
        start_here.value.as_ref(),
        gap_ledger.value.as_ref(),
        repo_exposure.value.as_ref(),
        diff_report.value.as_ref(),
        baseline_value,
        attempt_ledger.value.as_ref(),
    );

    let json_text = render_pr_evidence_summary_json(&summary_struct);
    let json_path = repo.join(PR_EVIDENCE_SUMMARY_JSON);
    write_parented_file(&json_path, PR_EVIDENCE_SUMMARY_JSON, json_text.as_bytes())?;
    println!("Wrote {PR_EVIDENCE_SUMMARY_JSON}");

    let md_text = render_evidence_summary_md(&summary_struct);
    let md_path = repo.join(PR_EVIDENCE_SUMMARY_MD);
    write_parented_file(&md_path, PR_EVIDENCE_SUMMARY_MD, md_text.as_bytes())?;
    println!("Wrote {PR_EVIDENCE_SUMMARY_MD}");
    Ok(())
}

fn summary_text(repo: &Path) -> String {
    let pr_evidence = load_json(repo, PR_EVIDENCE_JSON);
    let review_comments = load_json(repo, REVIEW_COMMENTS_JSON);
    let start_here = load_json(repo, START_HERE_JSON);
    render_pr_evidence_summary(&SummaryRenderInput {
        repo,
        pr_evidence_json: PR_EVIDENCE_JSON,
        review_comments_json: REVIEW_COMMENTS_JSON,
        start_here_json: START_HERE_JSON,
        pr_evidence_md: PR_EVIDENCE_MD,
        review_comments_md: REVIEW_COMMENTS_MD,
        start_here_md: START_HERE_MD,
        pr_summary_md: PR_SUMMARY_MD,
        pr_evidence: &pr_evidence,
        review_comments: &review_comments,
        start_here: &start_here,
    })
}

fn parse_options(args: &[String]) -> Result<SummaryOptions, String> {
    let mut root = PathBuf::from(".");
    let mut check = false;
    let mut baseline: Option<String> = None;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--check" => check = true,
            "--root" | "--baseline" => {
                index += 1;
                let path = expect_value(args, index, arg)?;
                if path.trim().is_empty() || path.starts_with('-') {
                    return Err(format!("pr-summary {arg} requires a non-empty path value"));
                }
                #[cfg(windows)]
                {
                    let parsed = Path::new(path);
                    if !parsed.is_absolute()
                        && (parsed.has_root()
                            || matches!(
                                parsed.components().next(),
                                Some(std::path::Component::Prefix(_))
                            ))
                    {
                        return Err(format!(
                            "pr-summary {arg} requires a fully qualified or ordinary relative path; \
                             Windows partially qualified paths are not supported"
                        ));
                    }
                }
                if arg == "--root" {
                    root = PathBuf::from(path);
                } else {
                    baseline = Some(path.to_string());
                }
            }
            other => return Err(unknown_argument("pr-summary", other)),
        }
        index += 1;
    }
    Ok(SummaryOptions {
        root,
        check,
        baseline,
    })
}

fn print_help() {
    println!("{PR_SUMMARY_HELP}");
}

/// Help body for `ripr pr-summary`. Also the flag source for unknown-argument
/// suggestions; keep accepted flags on option-list lines.
pub(crate) const PR_SUMMARY_HELP: &str = "\
Write the PR evidence summary from existing RIPR artifacts.

Usage: ripr pr-summary [--root <path>] [--check] [--baseline <before.json>]

Options:
  --root <path>        Select the artifact repository (default: current directory).
  --check              Verify the existing summary is up to date.
  --baseline <path>    Before-snapshot JSON for gap delta counts, relative to the selected root.

Relative artifact inputs and all outputs are anchored under --root.
On Windows, drive-relative and root-relative paths (C:repo or \\repo) are rejected.

Outputs:
  target/ripr/pr/summary.md  — legacy PR evidence summary (Markdown)
  target/ripr/reports/pr-evidence-summary.json  — v1 evidence summary (JSON)
  target/ripr/reports/pr-evidence-summary.md  — v1 evidence summary (Markdown panel)
";

fn check_summary(path: &Path, expected: &str) -> Result<(), String> {
    let actual = fs::read_to_string(path)
        .map_err(|err| format!("missing or unreadable {PR_SUMMARY_MD}: {err}"))?;
    if actual == expected {
        println!("PR evidence summary contract ok: {PR_SUMMARY_MD}");
        Ok(())
    } else {
        Err(format!("{PR_SUMMARY_MD} is stale; run `ripr pr-summary`"))
    }
}

fn write_summary(path: &Path, summary: &str) -> Result<(), String> {
    write_parented_file(path, PR_SUMMARY_MD, summary)?;
    println!("Wrote {PR_SUMMARY_MD}");
    Ok(())
}

/// Resolve a relative selected root once against the invocation directory.
/// Absolute selections do not depend on the process working directory.
fn repo_root(root: &Path) -> Result<PathBuf, String> {
    if root.is_absolute() {
        return Ok(root.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(root))
        .map_err(|err| format!("failed to determine working directory: {err}"))
}

fn write_parented_file(path: &Path, label: &str, contents: impl AsRef<[u8]>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create parent dir for {label}: {err}"))?;
    }
    crate::output::file_write::write(path, contents.as_ref())
        .map_err(|err| format!("failed to write {label}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_check_only() {
        assert_eq!(
            parse_options(&["--check".to_string()]),
            Ok(SummaryOptions {
                root: PathBuf::from("."),
                check: true,
                baseline: None
            })
        );
    }

    #[test]
    fn parse_accepts_baseline() {
        assert_eq!(
            parse_options(&["--baseline".to_string(), "before.json".to_string()]),
            Ok(SummaryOptions {
                root: PathBuf::from("."),
                check: false,
                baseline: Some("before.json".to_string())
            })
        );
    }

    #[test]
    fn parse_accepts_root_check_and_baseline() -> Result<(), String> {
        let args = [
            "--root",
            "selected répo",
            "--check",
            "--baseline",
            "before.json",
        ]
        .map(str::to_string);
        let options = parse_options(&args)?;
        if options.root.as_path() != Path::new("selected répo")
            || !options.check
            || options.baseline.as_deref() != Some("before.json")
        {
            return Err(format!("unexpected summary options: {options:?}"));
        }
        Ok(())
    }

    #[test]
    fn parse_rejects_missing_blank_and_flag_like_paths() -> Result<(), String> {
        for flag in ["--root", "--baseline"] {
            for value in [None, Some(""), Some("   "), Some("--check")] {
                let mut args = vec![flag.to_string()];
                if let Some(value) = value {
                    args.push(value.to_string());
                }
                match parse_options(&args) {
                    Err(error) if error.contains(flag) => {}
                    other => return Err(format!("malformed {args:?} accepted: {other:?}")),
                }
            }
        }
        Ok(())
    }

    #[test]
    fn parse_rejects_unknown_arg() -> Result<(), String> {
        match parse_options(&["--baselin".to_string()]) {
            Err(msg)
                if msg.contains("Did you mean `--baseline`?")
                    && msg.contains("Run `ripr pr-summary --help`.") =>
            {
                Ok(())
            }
            other => Err(format!(
                "expected scoped pr-summary suggestion, got {other:?}"
            )),
        }
    }

    fn unique_dir(label: &str) -> Result<PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock before epoch: {error}"))?
            .as_nanos();
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent).map_err(|error| format!("create temp parent: {error}"))?;
        Ok(parent.join(format!(
            "ripr-pr-summary-mod-{label}-{}-{stamp}",
            std::process::id()
        )))
    }

    fn fixture_root(label: &str) -> Result<PathBuf, String> {
        let root = unique_dir(label)?;
        fs::create_dir_all(&root).map_err(|error| format!("create root: {error}"))?;
        if !root.is_dir() {
            return Err(format!("fixture root not created: {}", root.display()));
        }
        Ok(root)
    }

    fn args_for(root: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec!["--root".to_string(), root.display().to_string()];
        args.extend(extra.iter().map(|arg| (*arg).to_string()));
        args
    }

    fn read(root: &Path, relative: &str) -> Result<String, String> {
        fs::read_to_string(root.join(relative)).map_err(|error| format!("read {relative}: {error}"))
    }

    #[test]
    fn parse_defaults_to_current_root_without_check_or_baseline() -> Result<(), String> {
        let options = parse_options(&[])?;
        if options
            != (SummaryOptions {
                root: PathBuf::from("."),
                check: false,
                baseline: None,
            })
        {
            return Err(format!("unexpected default options: {options:?}"));
        }
        Ok(())
    }

    #[test]
    fn parse_last_root_wins_regardless_of_flag_order() -> Result<(), String> {
        let args = ["--check", "--root", "a", "--root", "b"].map(str::to_string);
        let options = parse_options(&args)?;
        if options.root.as_path() != Path::new("b") || !options.check {
            return Err(format!("unexpected options: {options:?}"));
        }
        Ok(())
    }

    #[test]
    fn help_flag_short_circuits_before_root_validation_or_writes() -> Result<(), String> {
        let missing = unique_dir("help-missing")?;
        for flag in ["--help", "-h"] {
            // Positive: help succeeds even beside an otherwise-refused root.
            run_pr_summary(&args_for(&missing, &[flag]))?;
            // Help wins over an unknown argument too.
            run_pr_summary(&[flag.to_string(), "--bogus".to_string()])?;
        }
        if missing.exists() {
            return Err("help must not create the root".to_string());
        }
        // Negative control: without help the same arguments refuse.
        match run_pr_summary(&args_for(&missing, &[])) {
            Err(error) if error.contains("is not a directory") => Ok(()),
            other => Err(format!("missing root accepted without help: {other:?}")),
        }
    }

    #[test]
    fn unknown_argument_is_refused_by_run() -> Result<(), String> {
        let root = fixture_root("unknown-arg")?;
        match run_pr_summary(&args_for(&root, &["--bogus"])) {
            Err(error) if error.contains("--bogus") => {}
            other => return Err(format!("unknown argument accepted: {other:?}")),
        }
        if root.join(PR_SUMMARY_MD).exists() {
            return Err("refused run must not write".to_string());
        }
        Ok(())
    }

    #[test]
    fn missing_root_is_refused_without_creating_it() -> Result<(), String> {
        let missing = unique_dir("missing-root")?;
        if missing.exists() {
            return Err("precondition: root must not exist".to_string());
        }
        let Err(error) = run_pr_summary(&args_for(&missing, &[])) else {
            return Err("missing root accepted".to_string());
        };
        if !error.contains("is not a directory") || !error.contains(&missing.display().to_string())
        {
            return Err(format!("unexpected refusal: {error}"));
        }
        if missing.exists() {
            return Err("refusal must not conjure the root".to_string());
        }
        Ok(())
    }

    #[test]
    fn file_typed_root_is_refused_but_directory_root_succeeds() -> Result<(), String> {
        let dir = fixture_root("file-root")?;
        let file = dir.join("not-a-dir");
        fs::write(&file, "x").map_err(|error| format!("write file root: {error}"))?;
        if !file.is_file() {
            return Err("precondition: file root must be a file".to_string());
        }
        match run_pr_summary(&args_for(&file, &[])) {
            Err(error) if error.contains("is not a directory") => {}
            other => return Err(format!("file root accepted: {other:?}")),
        }
        if !file.is_file() {
            return Err("refusal must leave the file untouched".to_string());
        }
        // Positive control: the containing directory is a valid root.
        run_pr_summary(&args_for(&dir, &[]))?;
        if !dir.join(PR_SUMMARY_MD).is_file() {
            return Err("valid root did not receive the summary".to_string());
        }
        Ok(())
    }

    #[test]
    fn run_writes_all_three_outputs_for_an_empty_root() -> Result<(), String> {
        let root = fixture_root("empty-root")?;
        run_pr_summary(&args_for(&root, &[]))?;

        let legacy = read(&root, PR_SUMMARY_MD)?;
        if !legacy.starts_with("# PR Evidence Summary\n\n## Start Here\n\n")
            || !legacy.contains("- start-here JSON: missing\n")
            || !legacy.ends_with(
                "\n_This summary is generated from diff-scoped artifacts. Do not copy it into public badge state._\n",
            )
        {
            return Err(format!("unexpected legacy summary:\n{legacy}"));
        }
        let json_text = read(&root, PR_EVIDENCE_SUMMARY_JSON)?;
        serde_json::from_str::<serde_json::Value>(&json_text)
            .map_err(|error| format!("summary JSON invalid: {error}"))?;
        let md = read(&root, PR_EVIDENCE_SUMMARY_MD)?;
        if !md.starts_with("# PR Evidence Summary v1\n\n**Run Status**: ") {
            return Err(format!("unexpected v1 markdown:\n{md}"));
        }
        Ok(())
    }

    #[test]
    fn invalid_start_here_json_is_reported_not_treated_as_missing() -> Result<(), String> {
        let missing_root = fixture_root("start-missing")?;
        run_pr_summary(&args_for(&missing_root, &[]))?;
        let missing = read(&missing_root, PR_SUMMARY_MD)?;

        let root = fixture_root("start-invalid")?;
        let start = root.join(START_HERE_JSON);
        fs::create_dir_all(start.parent().ok_or("start-here has no parent")?)
            .map_err(|error| format!("create reports dir: {error}"))?;
        fs::write(&start, "{ not json").map_err(|error| format!("write start-here: {error}"))?;
        if serde_json::from_str::<serde_json::Value>("{ not json").is_ok() {
            return Err("precondition: fixture must be invalid JSON".to_string());
        }
        run_pr_summary(&args_for(&root, &[]))?;
        let invalid = read(&root, PR_SUMMARY_MD)?;
        if !invalid.contains("- start-here JSON: invalid: ") {
            return Err(format!("invalid start-here not disclosed:\n{invalid}"));
        }
        // Negative control: the states are distinguished in both directions.
        if missing.contains("start-here JSON: invalid")
            || invalid.contains("- start-here JSON: missing")
        {
            return Err("input states are not distinguished".to_string());
        }
        Ok(())
    }

    #[test]
    fn check_mode_accepts_fresh_summary_and_rejects_stale_without_rewriting() -> Result<(), String>
    {
        let root = fixture_root("check-stale")?;
        run_pr_summary(&args_for(&root, &[]))?;
        // Positive control: freshly written summary passes --check.
        run_pr_summary(&args_for(&root, &["--check"]))?;

        let path = root.join(PR_SUMMARY_MD);
        fs::write(&path, "tampered\n").map_err(|error| format!("tamper: {error}"))?;
        let Err(error) = run_pr_summary(&args_for(&root, &["--check"])) else {
            return Err("stale summary passed --check".to_string());
        };
        if error != format!("{PR_SUMMARY_MD} is stale; run `ripr pr-summary`") {
            return Err(format!("unexpected stale message: {error}"));
        }
        if read(&root, PR_SUMMARY_MD)? != "tampered\n" {
            return Err("--check must not rewrite the summary".to_string());
        }
        Ok(())
    }

    #[test]
    fn check_mode_with_missing_summary_fails_and_writes_nothing() -> Result<(), String> {
        let root = fixture_root("check-missing")?;
        let Err(error) = run_pr_summary(&args_for(&root, &["--check"])) else {
            return Err("missing summary passed --check".to_string());
        };
        if !error.starts_with(&format!("missing or unreadable {PR_SUMMARY_MD}: ")) {
            return Err(format!("unexpected message: {error}"));
        }
        for relative in [
            PR_SUMMARY_MD,
            PR_EVIDENCE_SUMMARY_JSON,
            PR_EVIDENCE_SUMMARY_MD,
        ] {
            if root.join(relative).exists() {
                return Err(format!("failed --check wrote {relative}"));
            }
        }
        Ok(())
    }

    #[test]
    fn check_summary_compares_exactly() -> Result<(), String> {
        let root = fixture_root("check-exact")?;
        let path = root.join("summary.md");
        fs::write(&path, "body\n").map_err(|error| format!("write: {error}"))?;
        check_summary(&path, "body\n")?;
        for near in ["body", "body\n\n", "Body\n", ""] {
            match check_summary(&path, near) {
                Err(error) if error.contains("is stale") => {}
                other => return Err(format!("{near:?} accepted: {other:?}")),
            }
        }
        Ok(())
    }

    #[test]
    fn unusable_baseline_degrades_to_no_baseline() -> Result<(), String> {
        let root = fixture_root("baseline")?;
        run_pr_summary(&args_for(&root, &[]))?;
        let without = read(&root, PR_EVIDENCE_SUMMARY_JSON)?;
        fs::write(root.join("before.json"), "{ not json")
            .map_err(|error| format!("write baseline: {error}"))?;
        run_pr_summary(&args_for(&root, &["--baseline", "before.json"]))?;
        run_pr_summary(&args_for(&root, &["--baseline", "absent.json"]))?;
        if read(&root, PR_EVIDENCE_SUMMARY_JSON)? != without {
            return Err("unusable baseline changed the v1 JSON".to_string());
        }
        Ok(())
    }

    #[test]
    fn repo_root_keeps_absolute_and_anchors_relative_to_cwd() -> Result<(), String> {
        let _cwd = crate::testing::cwd_lock::hold_cwd();
        let absolute = unique_dir("repo-root")?;
        if repo_root(&absolute)? != absolute {
            return Err("absolute root must be returned unchanged".to_string());
        }
        let cwd = std::env::current_dir().map_err(|error| format!("cwd: {error}"))?;
        let relative = Path::new("some/relative-root");
        if relative.is_absolute() {
            return Err("precondition: fixture must be relative".to_string());
        }
        if repo_root(relative)? != cwd.join(relative) {
            return Err("relative root must join the working directory".to_string());
        }
        Ok(())
    }

    #[test]
    fn write_parented_file_creates_parents_and_refuses_blocked_parent() -> Result<(), String> {
        let root = fixture_root("write-parented")?;
        let nested = root.join("a/b/out.txt");
        write_parented_file(&nested, "a/b/out.txt", "hello")?;
        if read(&root, "a/b/out.txt")? != "hello" {
            return Err("nested write content mismatch".to_string());
        }
        // Negative control: a regular file where the parent directory belongs.
        fs::write(root.join("blocker"), "x").map_err(|error| format!("blocker: {error}"))?;
        if !root.join("blocker").is_file() {
            return Err("precondition: blocker must be a file".to_string());
        }
        match write_parented_file(&root.join("blocker/out.txt"), "blocker/out.txt", "x") {
            Err(error)
                if error.starts_with("failed to create parent dir for blocker/out.txt: ") =>
            {
                Ok(())
            }
            other => Err(format!("blocked parent accepted: {other:?}")),
        }
    }

    #[test]
    fn write_summary_failure_names_the_label() -> Result<(), String> {
        let root = fixture_root("write-summary-dir")?;
        let dest = root.join("dest");
        fs::create_dir_all(&dest).map_err(|error| format!("dest dir: {error}"))?;
        if !dest.is_dir() {
            return Err("precondition: destination must be a directory".to_string());
        }
        match write_summary(&dest, "x") {
            Err(error) if error.starts_with(&format!("failed to write {PR_SUMMARY_MD}: ")) => {
                Ok(())
            }
            other => Err(format!("directory destination accepted: {other:?}")),
        }
    }

    #[cfg(windows)]
    #[test]
    fn parse_rejects_windows_partially_qualified_paths() -> Result<(), String> {
        // These relative paths replace rather than extend the selected root.
        // Assemble generic drive fixtures at runtime, following the repository
        // local-context convention; no host-machine path is committed.
        let drive = 'C';
        let selected = PathBuf::from(format!(r"{drive}:\selected"));
        let before = format!(r"{drive}:\before.json");
        for (relative, expected) in [("C:repo", "C:repo"), (r"\before.json", before.as_str())] {
            let relative = Path::new(relative);
            if selected.join(relative) != Path::new(expected) || relative.is_absolute() {
                return Err(format!(
                    "Windows path replacement premise changed: {relative:?}"
                ));
            }
        }
        for flag in ["--root", "--baseline"] {
            for path in ["C:repo", "C:", r"\repo", "/repo"] {
                let args = [flag.to_string(), path.to_string()];
                match parse_options(&args) {
                    Err(error) if error.contains(flag) => {}
                    other => {
                        return Err(format!("partially qualified {args:?} accepted: {other:?}"));
                    }
                }
            }
            let absolute = format!(r"{drive}:\repo");
            let verbatim = format!(r"\\?\{drive}:\repo");
            for path in [
                absolute.as_str(),
                r"\\server\share\repo",
                verbatim.as_str(),
                r"\\?\UNC\server\share\repo",
            ] {
                if !Path::new(path).is_absolute() {
                    return Err(format!(
                        "Windows absolute fixture is not absolute: {path:?}"
                    ));
                }
                parse_options(&[flag.to_string(), path.to_string()])?;
            }
            for path in ["repo", "../repo"] {
                parse_options(&[flag.to_string(), path.to_string()])?;
            }
        }
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn parse_accepts_colons_in_relative_paths() -> Result<(), String> {
        for flag in ["--root", "--baseline"] {
            parse_options(&[flag.to_string(), "C:repo".to_string()])?;
        }
        Ok(())
    }
}

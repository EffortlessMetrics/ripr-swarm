//! `check-user-text`: plain-language lint over real default human output (#10697).
//!
//! The authority for the banned-term list and its plain replacements is
//! `docs/USER_TEXT_GLOSSARY.md`. This check scans what a user actually reads:
//!
//! 1. every committed default `human.txt` golden under `fixtures/**/expected/`;
//! 2. live default output of the built `ripr` binary: `check`, `pilot`, `doctor`,
//!    `status`, `continue`, `repair` and `agent` refusals, an `agent card`, and
//!    the help screens.
//!
//! Terms already present on `main` are held by a documented ceiling file,
//! `policy/user-text-baseline.toml`. A ceiling can only be lowered (`--ratchet`);
//! raising one needs a reviewed edit to that file. Excluded on purpose, and
//! printed in every report: `human-full.txt`, `--verbose`/`--format human-full`
//! output, JSON, and spec documents (internal vocabulary lives there).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BASELINE_PATH: &str = "policy/user-text-baseline.toml";
const REPORT_PATH: &str = "target/ripr/reports/user-text.md";
const GOLDEN_SURFACE: &str = "golden-human";
const GLOSSARY: &str = "docs/USER_TEXT_GLOSSARY.md";

/// Surfaces and files deliberately not scanned. Printed in every report.
const ALLOWLISTED_EXCLUSIONS: &[&str] = &[
    "fixtures/**/expected/human-full.txt (--format human-full)",
    "any output of --verbose / -v",
    "JSON, SARIF and other machine formats",
    "docs/specs/** and other internal reference documents",
];

struct Banned {
    id: &'static str,
    needles: &'static [&'static str],
}

/// The banned-term list. Needles are lowercase; an entry ending in `-` matches
/// as a prefix (for example `ripr-spec-0001`).
const BANNED: &[Banned] = &[
    Banned {
        id: "probe",
        needles: &["probe", "probes"],
    },
    Banned {
        id: "seam",
        needles: &["seam", "seams"],
    },
    Banned {
        id: "discriminator",
        needles: &["discriminator", "discriminators"],
    },
    Banned {
        id: "edit cage",
        needles: &["edit cage"],
    },
    Banned {
        id: "admitted",
        needles: &["admitted"],
    },
    Banned {
        id: "delegatable",
        needles: &["delegatable"],
    },
    Banned {
        id: "typed refusal",
        needles: &["typed refusal"],
    },
    Banned {
        id: "RIPR-SPEC-",
        needles: &["ripr-spec-"],
    },
    Banned {
        id: "state token",
        needles: &[
            "weakly_exposed",
            "reachable_unrevealed",
            "no_static_path",
            "infection_unknown",
            "propagation_unknown",
            "static_unknown",
            "weakly_gripped",
            "strongly_gripped",
            "ungripped",
            "partial_with_limitations",
            "complete_with_findings",
            "language_adapter_unavailable",
            "missing_artifacts",
            "missing_artifact",
            "unsupported_or_limited",
            "choose_item",
            "select_item",
            "allowed_edit_surface",
            "propagation_evidence_unknown",
            "identity_unnameable",
            "static_limit_kind",
        ],
    },
];

/// The one flag spelling that keeps the banned word (`--seam-id`).
const ALLOWED_FLAG: &str = "--seam-id";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TermCount {
    pub lines: usize,
    pub examples: Vec<String>,
}

pub(crate) type Counts = BTreeMap<(String, String), TermCount>;

fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Remove copy-paste identifiers before matching: the exact `--seam-id` flag
/// and finding IDs of the form `probe:<token>` / `repo-probe:<token>` (a
/// lowercase `probe:` glued to a non-space token, not preceded by a word
/// character). Prose such as `Probe: x`, `multi-seam` or `probe: x` is kept.
fn strip_identifiers(line: &str) -> String {
    let line = line.replace(ALLOWED_FLAG, " ");
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < line.len() {
        let rest = &line[index..];
        let at_boundary = index == 0 || !is_word_char(bytes[index - 1]);
        if at_boundary && let Some(after) = rest.strip_prefix("probe:") {
            let token = after
                .split_whitespace()
                .next()
                .filter(|_| after.chars().next().is_some_and(|c| !c.is_whitespace()));
            if let Some(token) = token {
                out.push(' ');
                index += "probe:".len() + token.len();
                continue;
            }
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

fn contains_needle(haystack_lower: &str, needle: &str) -> bool {
    let bytes = haystack_lower.as_bytes();
    let prefix_only = needle.ends_with('-');
    let mut start = 0usize;
    while let Some(idx) = haystack_lower[start..].find(needle) {
        let begin = start + idx;
        let end = begin + needle.len();
        let before_ok = begin == 0 || !is_word_char(bytes[begin - 1]);
        let after_ok = prefix_only || end == bytes.len() || !is_word_char(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        start = begin + 1;
        if start >= haystack_lower.len() {
            break;
        }
    }
    false
}

/// Banned term ids found on one line of output.
pub(crate) fn banned_terms_in_line(line: &str) -> Vec<&'static str> {
    let lower = strip_identifiers(line).to_ascii_lowercase();
    BANNED
        .iter()
        .filter(|banned| banned.needles.iter().any(|n| contains_needle(&lower, n)))
        .map(|banned| banned.id)
        .collect()
}

fn excerpt(line: &str) -> String {
    let trimmed = line.trim();
    let mut taken: String = trimmed.chars().take(120).collect();
    if trimmed.chars().count() > 120 {
        taken.push_str("...");
    }
    taken
}

pub(crate) fn scan_text(surface: &str, label: &str, text: &str, counts: &mut Counts) {
    for (idx, line) in text.lines().enumerate() {
        for term in banned_terms_in_line(line) {
            let entry = counts
                .entry((surface.to_string(), term.to_string()))
                .or_default();
            entry.lines += 1;
            if entry.examples.len() < 3 {
                entry
                    .examples
                    .push(format!("{label}:{}: {}", idx + 1, excerpt(line)));
            }
        }
    }
}

/// Default `human.txt` goldens: `<root>/fixtures/**/expected/human.txt`.
pub(crate) fn golden_human_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries =
            fs::read_dir(dir).map_err(|err| format!("read dir {}: {err}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|err| format!("read dir {}: {err}", dir.display()))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|err| format!("stat {}: {err}", path.display()))?;
            if kind.is_dir() {
                walk(&path, out)?;
            } else if path.file_name().is_some_and(|name| name == "human.txt")
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == "expected")
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(&root.join("fixtures"), &mut out)?;
    out.sort();
    Ok(out)
}

pub(crate) fn scan_goldens(root: &Path, counts: &mut Counts) -> Result<usize, String> {
    let files = golden_human_files(root)?;
    for path in &files {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("read golden {}: {err}", path.display()))?;
        let label = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        scan_text(GOLDEN_SURFACE, &label, &text, counts);
    }
    Ok(files.len())
}

// ---------------------------------------------------------------- baseline

pub(crate) type Baseline = BTreeMap<(String, String), usize>;

pub(crate) fn parse_baseline(text: &str) -> Result<Baseline, String> {
    let value: toml::Value =
        toml::from_str(text).map_err(|err| format!("{BASELINE_PATH} is not valid TOML: {err}"))?;
    if value.get("schema_version").and_then(toml::Value::as_str) != Some("1.0") {
        return Err(format!("{BASELINE_PATH} needs schema_version = \"1.0\""));
    }
    let mut baseline = Baseline::new();
    let rows = value
        .get("ceiling")
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for row in rows {
        let surface = row.get("surface").and_then(toml::Value::as_str);
        let term = row.get("term").and_then(toml::Value::as_str);
        let max = row.get("max_lines").and_then(toml::Value::as_integer);
        let (Some(surface), Some(term), Some(max)) = (surface, term, max) else {
            return Err(format!(
                "{BASELINE_PATH}: each [[ceiling]] needs surface, term and max_lines"
            ));
        };
        if !BANNED.iter().any(|banned| banned.id == term) {
            return Err(format!("{BASELINE_PATH}: unknown term `{term}`"));
        }
        let max = usize::try_from(max).map_err(|err| {
            format!("{BASELINE_PATH}: max_lines for {surface}/{term} is out of range: {err}")
        })?;
        if baseline
            .insert((surface.to_string(), term.to_string()), max)
            .is_some()
        {
            return Err(format!(
                "{BASELINE_PATH}: duplicate ceiling for {surface}/{term}"
            ));
        }
    }
    Ok(baseline)
}

fn render_baseline(baseline: &Baseline) -> String {
    let mut out = String::from(
        "# Ceilings for banned plain-language terms in default human output (#10697).\n\
         # Each number is the count of output lines containing the term on `main`.\n\
         # `cargo xtask check-user-text` fails when a count rises above its ceiling.\n\
         # Lower a ceiling with `cargo xtask check-user-text --ratchet`; never raise one\n\
         # without a reviewed reason in the PR. A term with no row has a ceiling of 0.\n\
         # See docs/USER_TEXT_GLOSSARY.md.\n\
         schema_version = \"1.0\"\n",
    );
    for ((surface, term), max) in baseline {
        out.push_str(&format!(
            "\n[[ceiling]]\nsurface = \"{surface}\"\nterm = \"{term}\"\nmax_lines = {max}\n"
        ));
    }
    out
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct Evaluation {
    pub violations: Vec<String>,
    pub lowerable: Vec<String>,
}

pub(crate) fn evaluate(counts: &Counts, baseline: &Baseline) -> Evaluation {
    let mut evaluation = Evaluation::default();
    for ((surface, term), found) in counts {
        let ceiling = baseline
            .get(&(surface.clone(), term.clone()))
            .copied()
            .unwrap_or(0);
        if found.lines > ceiling {
            let plain = format!(
                "`{term}` in {surface}: {} line(s), ceiling {ceiling}. First hits: {}",
                found.lines,
                found.examples.join(" | ")
            );
            evaluation.violations.push(plain);
        } else if found.lines < ceiling {
            evaluation.lowerable.push(format!(
                "`{term}` in {surface}: {} line(s), ceiling {ceiling}",
                found.lines
            ));
        }
    }
    for ((surface, term), ceiling) in baseline {
        if *ceiling > 0 && !counts.contains_key(&(surface.clone(), term.clone())) {
            evaluation.lowerable.push(format!(
                "`{term}` in {surface}: 0 line(s), ceiling {ceiling}"
            ));
        }
    }
    evaluation
}

/// Lower every ceiling to the observed count; never raise or add one.
pub(crate) fn ratcheted(counts: &Counts, baseline: &Baseline) -> Baseline {
    baseline
        .iter()
        .map(|(key, ceiling)| {
            let observed = counts.get(key).map_or(0, |found| found.lines);
            (key.clone(), (*ceiling).min(observed))
        })
        .filter(|(_, ceiling)| *ceiling > 0)
        .collect()
}

/// The baseline `--ratchet` would write, or `None` when any count is above its
/// ceiling: a failing run must not half-update the file, and the command's
/// result must describe the same tree the file is written for.
pub(crate) fn ratchet_outcome(counts: &Counts, baseline: &Baseline) -> Option<Baseline> {
    if evaluate(counts, baseline).violations.is_empty() {
        Some(ratcheted(counts, baseline))
    } else {
        None
    }
}

// ------------------------------------------------------------- live surface

struct LiveRun {
    surface: String,
    args: Vec<String>,
    must_contain: &'static str,
    cwd: PathBuf,
}

struct Scratch {
    plain_dir: PathBuf,
    repo: PathBuf,
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args([
            "-c",
            "user.email=user-text@example.invalid",
            "-c",
            "user.name=user-text",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|err| format!("git {args:?}: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("write {}: {err}", path.display()))
}

fn build_scratch(base: &Path) -> Result<Scratch, String> {
    let plain_dir = base.join("plain");
    fs::create_dir_all(&plain_dir).map_err(|err| format!("create scratch: {err}"))?;
    let repo = base.join("repo");
    write_file(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    write_file(
        &repo.join("src/lib.rs"),
        "pub fn discount(amount: i32) -> i32 {\n    if amount >= 100 { amount - 10 } else { amount }\n}\n",
    )?;
    write_file(
        &repo.join("tests/t.rs"),
        "#[test]\nfn t() {\n    assert!(demo::discount(5) == 5);\n}\n",
    )?;
    run_git(&repo, &["init", "-q", "-b", "main"])?;
    run_git(&repo, &["add", "-A"])?;
    run_git(&repo, &["commit", "-q", "-m", "init"])?;
    write_file(
        &repo.join("src/lib.rs"),
        "pub fn discount(amount: i32) -> i32 {\n    if amount > 100 { amount - 10 } else { amount }\n}\n",
    )?;
    Ok(Scratch { plain_dir, repo })
}

fn ripr_output(bin: &Path, cwd: &Path, args: &[String]) -> Result<String, String> {
    let output = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .output()
        .map_err(|err| format!("run {} {args:?}: {err}", bin.display()))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

fn strs(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

/// Gap ID from `ripr pilot` output: the `gap <id>: ...` line.
fn gap_id_from_pilot(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("gap "))
        .filter_map(|rest| rest.split(':').next())
        .find(|id| id.len() >= 8 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_string)
}

fn live_runs(scratch: &Scratch) -> Vec<LiveRun> {
    let repo = &scratch.repo;
    let mut runs = vec![
        LiveRun {
            surface: "help-top".into(),
            args: strs(&["--help"]),
            must_contain: "What are you trying to do?",
            cwd: scratch.plain_dir.clone(),
        },
        LiveRun {
            surface: "help-all".into(),
            args: strs(&["help", "--all"]),
            must_contain: "ripr \u{2014} complete command reference.",
            cwd: scratch.plain_dir.clone(),
        },
        LiveRun {
            surface: "check-not-git".into(),
            args: strs(&["check"]),
            must_contain: "not inside a Git work tree",
            cwd: scratch.plain_dir.clone(),
        },
    ];
    for (command, usage) in [
        ("check", "Usage: ripr check [OPTIONS]"),
        ("pilot", "Usage: ripr pilot [--root PATH]"),
        ("doctor", "Usage: ripr doctor [--root PATH]"),
        ("status", "Usage: ripr status [--attempt ID]"),
        ("repair", "Usage: ripr repair [<item>]"),
        ("continue", "Usage: ripr continue [--attempt ID]"),
        ("explain", "Usage: ripr explain [--root PATH]"),
        ("context", "Print the per-change context packet"),
        ("agent", "Usage: ripr agent <subcommand>"),
    ] {
        runs.push(LiveRun {
            surface: format!("help-{command}"),
            args: strs(&["help", command]),
            must_contain: usage,
            cwd: scratch.plain_dir.clone(),
        });
    }
    let repo_runs: [(&str, &[&str], &'static str); 9] = [
        ("check", &["check"], "Summary:"),
        ("pilot", &["pilot"], "RIPR pilot complete"),
        ("doctor", &["doctor"], "ripr doctor (analysis)"),
        ("status", &["status"], "# RIPR Agent Status"),
        ("repair-refusal", &["repair"], "Nothing was started"),
        (
            "agent-card-unknown-id",
            &["agent", "card", "--seam-id", "nope"],
            "agent card seam_id nope was not found.",
        ),
        (
            "agent-packet-refusal",
            &["agent", "packet"],
            "ripr: agent packet requires --seam-id or --gap-ledger with --gap-id",
        ),
        (
            "agent-repair-refusal",
            &["agent", "repair", "--phase", "before"],
            "ripr: agent repair --phase before requires --seam-id <id>;",
        ),
        (
            "continue-refusal",
            &["continue"],
            "ripr: no current attempt to continue",
        ),
    ];
    for (surface, args, must_contain) in repo_runs {
        runs.push(LiveRun {
            surface: surface.into(),
            args: strs(args),
            must_contain,
            cwd: repo.clone(),
        });
    }
    runs
}

fn resolve_ripr_binary(explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        let path = PathBuf::from(path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("--ripr {} is not a file", path.display()))
        };
    }
    let exe = std::env::current_exe().map_err(|err| format!("resolve xtask binary: {err}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "xtask binary has no parent directory".to_string())?;
    let release = dir.file_name().is_some_and(|name| name == "release");
    let mut build = Command::new("cargo");
    build.args(["build", "-q", "-p", "ripr", "--bin", "ripr"]);
    if release {
        build.arg("--release");
    }
    let status = build
        .status()
        .map_err(|err| format!("build ripr for check-user-text: {err}"))?;
    if !status.success() {
        return Err(
            "cargo build -p ripr --bin ripr failed (instrument failure, not a pass)".into(),
        );
    }
    let name = if cfg!(windows) { "ripr.exe" } else { "ripr" };
    let path = dir.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("built ripr binary not found at {}", path.display()))
    }
}

/// Scratch directory outside the checkout. `.cargo/config.toml` forces TMPDIR
/// into the repository, and `ripr` resolves its root by walking up to the
/// nearest workspace `Cargo.toml`, so a scratch dir inside the checkout would
/// not be a separate repository (nor a non-Git directory).
fn scratch_base(root: &Path) -> Result<PathBuf, String> {
    let name = format!("ripr-user-text-{}", std::process::id());
    let mut candidates = Vec::new();
    if let Some(explicit) = std::env::var_os("RIPR_USER_TEXT_SCRATCH") {
        candidates.push(PathBuf::from(explicit));
    }
    candidates.push(std::env::temp_dir());
    if cfg!(unix) {
        candidates.push(PathBuf::from("/tmp"));
    }
    for candidate in candidates {
        let Ok(absolute) = std::path::absolute(&candidate) else {
            continue;
        };
        if !absolute.starts_with(root) {
            return Ok(absolute.join(name));
        }
    }
    Err("no scratch directory outside the checkout; set RIPR_USER_TEXT_SCRATCH".to_string())
}

fn scan_live(bin: &Path, root: &Path, counts: &mut Counts) -> Result<Vec<String>, String> {
    let base = scratch_base(root)?;
    if base.exists() {
        fs::remove_dir_all(&base).map_err(|err| format!("clear {}: {err}", base.display()))?;
    }
    let scratch = build_scratch(&base)?;
    let mut scanned = Vec::new();
    let mut runs = live_runs(&scratch);
    // Pilot first so its gap ID can drive a real `agent card`.
    let mut pilot_text = String::new();
    let mut index = 0usize;
    while index < runs.len() {
        let run = &runs[index];
        let text = ripr_output(bin, &run.cwd, &run.args)?;
        if !text.contains(run.must_contain) {
            return Err(format!(
                "live surface `{}` did not produce the expected output (missing `{}`); the scratch setup or binary is wrong, not a pass. Output began: {}",
                run.surface,
                run.must_contain,
                excerpt(&text)
            ));
        }
        if run.surface == "pilot" {
            pilot_text = text.clone();
        }
        scan_text(&run.surface, &run.surface, &text, counts);
        scanned.push(run.surface.clone());
        index += 1;
    }
    // After the pilot, with target/ ignored, the commit-first refusal and a real card.
    write_file(&scratch.repo.join(".gitignore"), "/target/\n")?;
    let gap = gap_id_from_pilot(&pilot_text)
        .ok_or_else(|| "pilot output named no gap ID; scratch setup is wrong".to_string())?;
    runs.clear();
    for (surface, args, must_contain) in [
        (
            "repair-commit-first",
            vec!["repair".to_string()],
            "committed first",
        ),
        (
            "agent-card",
            vec![
                "agent".to_string(),
                "card".to_string(),
                "--seam-id".to_string(),
                gap,
            ],
            "Repair brief",
        ),
    ] {
        let text = ripr_output(bin, &scratch.repo, &args)?;
        if !text.contains(must_contain) {
            return Err(format!(
                "live surface `{surface}` did not produce the expected output (missing `{must_contain}`). Output began: {}",
                excerpt(&text)
            ));
        }
        scan_text(surface, surface, &text, counts);
        scanned.push(surface.to_string());
    }
    let _ = fs::remove_dir_all(&base);
    Ok(scanned)
}

// ------------------------------------------------------------------- report

fn render_report(
    status: &str,
    golden_files: usize,
    live: &[String],
    counts: &Counts,
    baseline: &Baseline,
    evaluation: &Evaluation,
) -> String {
    let mut out = format!("# check-user-text report\n\nStatus: {status}\n\n");
    if !evaluation.lowerable.is_empty() {
        out.push_str(&format!(
            "NOTE: {} ceiling(s) are higher than the current count and can be lowered. Run `cargo xtask check-user-text --ratchet` and commit the result (advisory; this does not fail the check):\n",
            evaluation.lowerable.len()
        ));
        for item in &evaluation.lowerable {
            out.push_str(&format!("- {item}\n"));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "Glossary and banned-term list: `{GLOSSARY}`. Ceilings: `{BASELINE_PATH}`.\n\n"
    ));
    out.push_str(&format!(
        "Scanned: {golden_files} default `human.txt` golden(s); {} live surface(s): {}\n\n",
        live.len(),
        live.join(", ")
    ));
    out.push_str("Not scanned (explicit allowlist):\n");
    for item in ALLOWLISTED_EXCLUSIONS {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("\n| surface | term | lines | ceiling |\n| --- | --- | ---: | ---: |\n");
    for ((surface, term), found) in counts {
        let ceiling = baseline
            .get(&(surface.clone(), term.clone()))
            .copied()
            .unwrap_or(0);
        out.push_str(&format!(
            "| {surface} | {term} | {} | {ceiling} |\n",
            found.lines
        ));
    }
    if !evaluation.violations.is_empty() {
        out.push_str("\nAbove ceiling:\n");
        for violation in &evaluation.violations {
            out.push_str(&format!("- {violation}\n"));
        }
        out.push_str(&format!(
            "\nRepair: replace the term with the plain wording in `{GLOSSARY}`, then re-bless affected goldens (see the glossary's bless path).\n"
        ));
    }
    out
}

fn write_report(root: &Path, body: &str) {
    let path = root.join(REPORT_PATH);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, body);
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!(
            "failed to resolve repo root from {}",
            manifest_dir.display()
        )
    })
}

/// Paths changed against the base or the working tree, or an error.
fn changed_paths() -> Result<Vec<String>, String> {
    let mut changed = crate::changed_files_vs_base(Path::new("."))?;
    changed.extend(crate::git_changed_files(Path::new("."), "HEAD")?);
    Ok(changed)
}

/// Run the live half when forced, when a changed path can alter what the built
/// binary prints, or when the diff cannot be read (fail toward running).
pub(crate) fn live_half_decision(forced: bool, changed: Result<Vec<String>, String>) -> bool {
    forced
        || match changed {
            Ok(paths) => paths.iter().any(|path| live_trigger_path(path)),
            Err(_) => true,
        }
}

pub(crate) fn live_trigger_path(path: &str) -> bool {
    path.starts_with("crates/ripr/")
        || path == "Cargo.toml"
        || path == "Cargo.lock"
        || path.starts_with("rust-toolchain")
        || path.starts_with(".cargo/")
        || path == "xtask/src/policy/user_text.rs"
        || path == BASELINE_PATH
}

/// Always scans the live output too (check-pr and CI).
pub(crate) fn check_user_text_full() -> Result<(), String> {
    check_user_text_with_args(&["--full".to_string()])
}

pub(crate) fn check_user_text() -> Result<(), String> {
    check_user_text_with_args(&[])
}

pub(crate) fn check_user_text_with_args(args: &[String]) -> Result<(), String> {
    let mut ripr: Option<String> = None;
    let mut ratchet = false;
    let mut full = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--ripr" => {
                ripr = Some(
                    iter.next()
                        .ok_or_else(|| "--ripr needs a path".to_string())?
                        .clone(),
                );
            }
            "--ratchet" => ratchet = true,
            "--full" => full = true,
            other => {
                return Err(format!(
                    "unknown argument `{other}`; usage: cargo xtask check-user-text [--ripr <path>] [--ratchet] [--full]"
                ));
            }
        }
    }
    let root = repo_root()?;
    let baseline_text = fs::read_to_string(root.join(BASELINE_PATH))
        .map_err(|err| format!("read {BASELINE_PATH}: {err}"))?;
    let baseline = parse_baseline(&baseline_text)?;

    let mut counts = Counts::new();
    let golden_files = scan_goldens(&root, &mut counts)?;
    if golden_files == 0 {
        return Err(
            "check-user-text found no default human.txt goldens; zero subjects is not a pass"
                .into(),
        );
    }
    // The live half builds `ripr` and runs ~25 commands. Run it when forced
    // (`--full`, `--ripr`, `--ratchet`, check-pr/CI) or when the change can
    // alter live output; a docs-only change scans the goldens only.
    let run_live = live_half_decision(full || ratchet || ripr.is_some(), changed_paths());
    let (live, baseline) = if run_live {
        let bin = resolve_ripr_binary(ripr.as_deref())?;
        (scan_live(&bin, &root, &mut counts)?, baseline)
    } else {
        println!(
            "check-user-text: live output half skipped, no ripr source or baseline change vs origin/main (golden half only; use --full to force)"
        );
        let golden_only = baseline
            .into_iter()
            .filter(|((surface, _), _)| surface == GOLDEN_SURFACE)
            .collect();
        (Vec::new(), golden_only)
    };

    let evaluation = evaluate(&counts, &baseline);
    let status = if evaluation.violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let report = render_report(status, golden_files, &live, &counts, &baseline, &evaluation);
    write_report(&root, &report);
    print!("{report}");
    if ratchet {
        if let Some(lowered) = ratchet_outcome(&counts, &baseline) {
            fs::write(root.join(BASELINE_PATH), render_baseline(&lowered))
                .map_err(|err| format!("write {BASELINE_PATH}: {err}"))?;
            println!("\nceilings lowered where counts dropped; none raised.");
        } else {
            println!("\n--ratchet did not rewrite {BASELINE_PATH}: a count is above its ceiling.");
        }
    }
    if !evaluation.lowerable.is_empty() {
        eprintln!(
            "check-user-text: NOTE {} ceiling(s) can be lowered; run `cargo xtask check-user-text --ratchet`",
            evaluation.lowerable.len()
        );
    }
    if evaluation.violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} plain-language term count(s) rose above their ceiling; see {REPORT_PATH} and {GLOSSARY}",
            evaluation.violations.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(surface: &str, text: &str) -> Counts {
        let mut counts = Counts::new();
        scan_text(surface, "seed", text, &mut counts);
        counts
    }

    #[test]
    fn every_banned_term_is_detected_and_plain_text_is_not() {
        for (line, term) in [
            ("Summary: 1 probe checked", "probe"),
            ("3 seams found", "seam"),
            ("Missing discriminator (1 of 3)", "discriminator"),
            ("edit cage: allowed=[tests/t.rs]", "edit cage"),
            ("no admitted test-target evidence", "admitted"),
            ("the packet is complete and delegatable", "delegatable"),
            ("exit 3 blocking decision or typed refusal", "typed refusal"),
            ("proof: RIPR-SPEC-0026", "RIPR-SPEC-"),
            ("Stop reasons: propagation_evidence_unknown", "state token"),
            ("grip (weakly_gripped)", "state token"),
        ] {
            assert_eq!(banned_terms_in_line(line), vec![term], "line: {line}");
        }
        for plain in [
            "Missing check (1 of 2): amount == 100",
            "ripr agent repair --seam-id 2377 --phase before",
            "ripr explain probe:src_lib.rs:predicate:9aa6fca0",
            "ripr explain repo-probe:src_lib.rs:predicate:9aa6fca0",
            "a test such as discount_boundary_discriminator in tests/t.rs",
            "Static exposure: weak (severity: warning)",
        ] {
            assert!(
                banned_terms_in_line(plain).is_empty(),
                "plain line flagged: {plain}"
            );
        }
    }

    /// Negative experiment (#10697): a seeded banned term in default human
    /// output exceeds a zero ceiling and fails the check; removing it passes.
    #[test]
    fn seeded_banned_term_fails_and_clean_text_passes() {
        let baseline = Baseline::new();
        let seeded = scan(GOLDEN_SURFACE, "Summary: 1 probe checked\n");
        let failing = evaluate(&seeded, &baseline);
        assert_eq!(failing.violations.len(), 1, "{failing:?}");
        assert!(failing.violations[0].contains("`probe`"));

        let clean = scan(GOLDEN_SURFACE, "Summary: 1 changed behavior checked\n");
        assert_eq!(evaluate(&clean, &baseline), Evaluation::default());
    }

    #[test]
    fn identifier_exemptions_are_narrow() {
        for flagged in [
            "a multi-seam finding",
            "Probe: src/lib.rs",
            "probe: not an id",
            "use --seam-id now with a seam",
        ] {
            assert!(
                !banned_terms_in_line(flagged).is_empty(),
                "should be flagged: {flagged}"
            );
        }
        for exempt in [
            "ripr agent repair --seam-id 2377 --phase before",
            "(`probe:src_lib.rs:predicate:9aa6` or `repo-probe:src_lib.rs:x:1`)",
            "ripr explain probe:src_lib.rs:predicate:9aa6fca0",
        ] {
            assert!(
                banned_terms_in_line(exempt).is_empty(),
                "should be exempt: {exempt}"
            );
        }
    }

    #[test]
    fn live_half_runs_only_for_output_affecting_paths() {
        for path in [
            "crates/ripr/src/cli/help.rs",
            "crates/ripr/Cargo.toml",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ] {
            assert!(live_trigger_path(path), "{path}");
        }
        assert!(live_half_decision(true, Ok(vec![])));
        assert!(live_half_decision(false, Err("git failed".into())));
        assert!(live_half_decision(
            false,
            Ok(vec!["crates/ripr/src/x.rs".into()])
        ));
        assert!(!live_half_decision(
            false,
            Ok(vec![
                "docs/README.md".into(),
                "fixtures/a/expected/human.txt".into()
            ])
        ));
        assert!(live_trigger_path(BASELINE_PATH));
        assert!(!live_trigger_path("docs/USER_TEXT_GLOSSARY.md"));
        assert!(!live_trigger_path("fixtures/x/expected/human.txt"));
    }

    #[test]
    fn a_ceiling_holds_existing_terms_but_not_new_growth() {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 1);
        let at_ceiling = scan(GOLDEN_SURFACE, "one seam here\n");
        assert!(evaluate(&at_ceiling, &baseline).violations.is_empty());
        let grown = scan(GOLDEN_SURFACE, "one seam here\nand another seam\n");
        assert_eq!(evaluate(&grown, &baseline).violations.len(), 1);
        let shrunk = scan(GOLDEN_SURFACE, "plain\n");
        let lowered = evaluate(&shrunk, &baseline);
        assert!(lowered.violations.is_empty());
        assert_eq!(lowered.lowerable.len(), 1);
    }

    #[test]
    fn ratchet_lowers_and_never_raises_or_adds() {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 5);
        baseline.insert((GOLDEN_SURFACE.into(), "probe".into()), 1);
        let mut counts = scan(GOLDEN_SURFACE, "a seam\nb seam\n");
        counts.extend(scan(GOLDEN_SURFACE, "c probe\nd probe\ne probe\n"));
        counts.extend(scan("doctor", "an edit cage\n"));
        let lowered = ratcheted(&counts, &baseline);
        assert_eq!(
            lowered.get(&(GOLDEN_SURFACE.into(), "seam".into())),
            Some(&2)
        );
        assert_eq!(
            lowered.get(&(GOLDEN_SURFACE.into(), "probe".into())),
            Some(&1)
        );
        assert_eq!(lowered.len(), 2, "ratchet must not add `edit cage`");
    }

    /// A live ceiling for a term the surface no longer prints (absent from the
    /// counts) is lowered to zero, which removes the row. Reintroducing the term
    /// later is then a violation against the implicit ceiling of 0.
    #[test]
    fn ratchet_removes_a_row_whose_term_is_gone_and_regrowth_then_fails() {
        let mut baseline = Baseline::new();
        baseline.insert(("help-repair".into(), "seam".into()), 10);
        baseline.insert(("help-repair".into(), "probe".into()), 2);
        let counts = scan("help-repair", "a probe\nb probe\n");
        let lowered = ratchet_outcome(&counts, &baseline).unwrap_or_default();
        assert_eq!(lowered.len(), 1, "{lowered:?}");
        assert_eq!(
            lowered.get(&("help-repair".into(), "probe".into())),
            Some(&2)
        );
        let regrown = scan("help-repair", "a seam\n");
        assert_eq!(evaluate(&regrown, &lowered).violations.len(), 1);
    }

    /// `--ratchet` must not rewrite the baseline while a count is above its
    /// ceiling; the run fails and the file stays as reviewed.
    #[test]
    fn ratchet_refuses_to_rewrite_while_a_count_is_above_its_ceiling() {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 1);
        baseline.insert((GOLDEN_SURFACE.into(), "probe".into()), 5);
        let mut counts = scan(GOLDEN_SURFACE, "a seam\nb seam\n");
        counts.extend(scan(GOLDEN_SURFACE, "c probe\n"));
        assert_eq!(ratchet_outcome(&counts, &baseline), None);
    }

    #[test]
    fn baseline_round_trips_and_rejects_unknown_terms() -> Result<(), String> {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 7);
        let reparsed = parse_baseline(&render_baseline(&baseline))?;
        assert_eq!(reparsed, baseline);
        let bad = "schema_version = \"1.0\"\n[[ceiling]]\nsurface = \"x\"\nterm = \"nonsense\"\nmax_lines = 1\n";
        let unknown = parse_baseline(bad)
            .err()
            .ok_or("unknown term must be rejected")?;
        assert!(unknown.contains("nonsense"), "{unknown}");
        let schemaless = parse_baseline("[[ceiling]]\n")
            .err()
            .ok_or("schema_version is required")?;
        assert!(schemaless.contains("schema_version"), "{schemaless}");
        Ok(())
    }

    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let root =
            std::env::temp_dir().join(format!("ripr-user-text-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        Ok(root)
    }

    /// The golden walk reads default `human.txt` only; `human-full.txt` is the
    /// allowlisted verbose surface and may keep internal vocabulary.
    #[test]
    fn golden_scan_reads_human_txt_and_skips_human_full() -> Result<(), String> {
        let root = temp_root("goldens")?;
        let expected = root.join("fixtures/case/expected");
        write_file(&expected.join("human.txt"), "Missing discriminator here\n")?;
        write_file(
            &expected.join("human-full.txt"),
            "probe seam discriminator\n",
        )?;
        write_file(&root.join("fixtures/case/notes/human.txt"), "probe\n")?;
        let mut counts = Counts::new();
        let scanned = scan_goldens(&root, &mut counts)?;
        let _ = fs::remove_dir_all(&root);
        assert_eq!(scanned, 1);
        let keys: Vec<_> = counts.keys().map(|(_, term)| term.as_str()).collect();
        assert_eq!(keys, vec!["discriminator"]);
        Ok(())
    }

    #[test]
    fn gap_id_is_read_from_pilot_output() {
        let text = "Top recommendation:\n  gap 2377230e7659a2e1: src/lib.rs:2 discount (x)\n";
        assert_eq!(gap_id_from_pilot(text).as_deref(), Some("2377230e7659a2e1"));
        assert_eq!(gap_id_from_pilot("nothing here"), None);
    }

    /// Hosted CI runs `ci-policy` -> `precommit_policy(true)`, which takes the
    /// collect-all path. That path must still run the full check, or a banned
    /// term increase would pass hosted CI while failing only locally.
    #[test]
    fn the_collect_all_ci_policy_path_runs_the_full_user_text_check() -> Result<(), String> {
        let source = fs::read_to_string(repo_root()?.join("xtask/src/main.rs"))
            .map_err(|err| format!("read main.rs: {err}"))?;
        let start = source
            .find("fn precommit_policy_remaining(")
            .ok_or("precommit_policy_remaining not found")?;
        let body = &source[start..];
        let end = body.find("\n}\n").ok_or("function end not found")?;
        let body = &body[..end];
        let local = body
            .find("check_user_text()?;")
            .ok_or("local path must run check_user_text()")?;
        let ci = body
            .find("policy::check_user_text_full()?;")
            .ok_or("collect_all path must run policy::check_user_text_full()")?;
        let guard = body[..ci]
            .rfind("if !collect_all")
            .ok_or("full check must sit in the else of `if !collect_all`")?;
        assert!(
            guard < local && local < ci,
            "full check must be the else arm"
        );
        Ok(())
    }

    /// The committed goldens must already sit within the committed ceilings,
    /// so a stale baseline cannot hide growth.
    #[test]
    fn committed_goldens_stay_within_the_committed_baseline() -> Result<(), String> {
        let root = repo_root()?;
        let baseline = parse_baseline(
            &fs::read_to_string(root.join(BASELINE_PATH))
                .map_err(|err| format!("read baseline: {err}"))?,
        )?;
        let mut counts = Counts::new();
        let scanned = scan_goldens(&root, &mut counts)?;
        if scanned == 0 {
            return Err("no default human.txt goldens were scanned".into());
        }
        let golden_only: Baseline = baseline
            .into_iter()
            .filter(|((surface, _), _)| surface == GOLDEN_SURFACE)
            .collect();
        let evaluation = evaluate(&counts, &golden_only);
        if evaluation.violations.is_empty() {
            Ok(())
        } else {
            Err(evaluation.violations.join("\n"))
        }
    }
}

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
//! Golden counts gate on the PR's own delta (#11374): a `(surface, term)` row
//! fails only when the tree has more banned lines than the merge-base tree, so
//! parallel merges cannot turn `main` red and PRs never edit the baseline. The
//! live surfaces are small and keep their absolute ceilings from
//! `policy/user-text-baseline.toml`. A ceiling can only be lowered (`--ratchet`).
//! A golden count above its ceiling is a reported warning, not a failure, when
//! the merge-base is readable; otherwise the absolute ceilings gate and the
//! report names `git fetch --deepen`. Excluded on purpose, and
//! printed in every report: `human-full.txt`, `--verbose`/`--format human-full`
//! output, JSON, and spec documents (internal vocabulary lives there).

use std::collections::{BTreeMap, BTreeSet};
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

/// The Evidence stage vocabulary the digest defines for the reader in its own
/// legend: the fifth stage name in the `Evidence:` chain and the legend's
/// definitions of it (`HUMAN_EVIDENCE_LEGEND`). These phrases are exempt because
/// the output glosses the word in plain language where it introduces it. Every
/// other use of the word, including prose such as "No strong discriminator",
/// stays counted.
const STAGE_VOCABULARY: &[&str] = &[
    "discriminator = assertion distinguishes intended from wrong behavior",
    "named discriminator absent from tests",
    "discriminator unconfirmed",
];

/// Remove the digest's stage vocabulary (see `STAGE_VOCABULARY`) before matching.
fn strip_stage_vocabulary(line: &str) -> String {
    let mut out = line.to_string();
    for phrase in STAGE_VOCABULARY {
        out = out.replace(phrase, " ");
    }
    if out.trim_start().starts_with("Evidence:") {
        out = out.replace(" · discriminator ", " · ");
    }
    out
}

/// Banned term ids found on one line of output.
pub(crate) fn banned_terms_in_line(line: &str) -> Vec<&'static str> {
    let lower = strip_identifiers(&strip_stage_vocabulary(line)).to_ascii_lowercase();
    BANNED
        .iter()
        .filter(|banned| banned.needles.iter().any(|n| contains_needle(&lower, n)))
        .map(|banned| banned.id)
        .collect()
}

/// The needles listed in the `BANNED` table of a `user_text.rs` source, read as
/// text so a base commit's list can be checked without building it. Needles sit
/// inside `needles: &[...]` groups as plain string literals.
pub(crate) fn needles_from_source(source: &str) -> Result<Vec<String>, String> {
    let start = source
        .find("const BANNED")
        .ok_or("no `const BANNED` table in the base user_text.rs")?;
    let table = &source[start..];
    let table = &table[..table.find("\n];").ok_or("unterminated BANNED table")?];
    let mut needles = Vec::new();
    let mut rest = table;
    while let Some(at) = rest.find("needles:") {
        rest = &rest[at + "needles:".len()..];
        let open = rest.find('[').ok_or("needles group without `[`")?;
        let close = rest.find(']').ok_or("needles group without `]`")?;
        let group = rest.get(open + 1..close).ok_or("malformed needles group")?;
        let mut parts = group.split('"');
        parts.next();
        while let Some(literal) = parts.next() {
            if literal.contains('\\') {
                return Err("escaped needle literal in the base BANNED table".into());
            }
            needles.push(literal.to_string());
            parts.next();
        }
        rest = &rest[close + 1..];
    }
    if needles.is_empty() {
        return Err("no needles found in the base BANNED table".into());
    }
    Ok(needles)
}

/// Lines a banned needle must stay detectable in: alone, inside a sentence,
/// punctuated and upper-cased. A prefix needle (ending in `-`) is probed with
/// a trailing identifier, which is how it is used.
fn needle_probe_lines(needle: &str) -> Vec<String> {
    let needle = if needle.ends_with('-') {
        format!("{needle}0001")
    } else {
        needle.to_string()
    };
    vec![
        needle.clone(),
        format!("a {needle} b"),
        format!("({needle}), then"),
        format!("A {} B", needle.to_ascii_uppercase()),
    ]
}

/// Needles the base banned that this matcher no longer detects. A PR cannot
/// qualify itself by editing the list or the matching code: both sides of the
/// golden delta use the head's matcher, so a weakened matcher shifts them
/// together and the delta hides it (#11385).
pub(crate) fn lost_needles(base_needles: &[String]) -> Vec<String> {
    base_needles
        .iter()
        .filter(|needle| {
            needle_probe_lines(needle)
                .iter()
                .any(|line| banned_terms_in_line(line).is_empty())
        })
        .cloned()
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
                if !kind.is_file() {
                    return Err(format!("golden {} is not a regular file", path.display()));
                }
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
        // One golden is one subject: the ceiling bounds the worst single file,
        // not a repository total that every landing golden would race to raise.
        let mut file_counts = Counts::new();
        scan_text(GOLDEN_SURFACE, &label, &text, &mut file_counts);
        for (key, found) in file_counts {
            let entry = counts.entry(key).or_default();
            if found.lines > entry.lines {
                *entry = found;
            }
        }
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
         # Each number is a ceiling on output lines containing the term; for `golden-human`\n\
         # it is the most lines any single golden may hold. `main` above a ceiling is\n\
         # reported as a warning.\n\
         # Golden rows gate each PR on its own per-file delta against the merge-base, so\n\
         # a PR never edits this file to pass; a golden over its ceiling is only a\n\
         # warning. Live-surface rows fail above their ceiling (raising one needs an\n\
         # explicit reviewed reason), as do golden rows when the merge-base is\n\
         # unreadable (shallow clone).\n\
         # Lower ceilings with `cargo xtask check-user-text --ratchet` (lowering only);\n\
         # never raise one. A term with no row has a ceiling of 0.\n\
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
    /// Golden counts above their absolute ceiling that the PR did not cause.
    pub warnings: Vec<String>,
}

/// Compare golden counts against the merge-base per `(path, term)` instead of
/// absolute ceilings (#11374). `head_paths` and `base_paths` are keyed by golden
/// path and term, so a removal in one file never offsets a rise in another, and
/// a path absent at the merge-base counts every banned line as an increase (a
/// rename that keeps old jargon is a deliberate false red, the safe direction).
/// Over-ceiling golden totals are warnings; every other surface keeps its
/// absolute ceiling.
pub(crate) fn evaluate_delta(
    counts: &Counts,
    baseline: &Baseline,
    head_paths: &Counts,
    base_paths: &BTreeMap<(String, String), usize>,
    base_files: &BTreeSet<String>,
) -> Evaluation {
    let absolute = evaluate(counts, baseline);
    let golden_prefix = format!("in {GOLDEN_SURFACE}:");
    let mut evaluation = Evaluation {
        violations: Vec::new(),
        lowerable: absolute.lowerable,
        warnings: Vec::new(),
    };
    for ((path, term), found) in head_paths {
        let before = base_paths
            .get(&(path.clone(), term.clone()))
            .copied()
            .unwrap_or(0);
        if found.lines > before {
            let origin = if base_files.contains(path) {
                format!("{before} at the merge-base")
            } else {
                "new path, 0 at the merge-base".to_string()
            };
            evaluation.violations.push(format!(
                "`{term}` in {GOLDEN_SURFACE} {path}: {} line(s), {origin} (+{}). First hits: {}",
                found.lines,
                found.lines - before,
                found.examples.join(" | ")
            ));
        }
    }
    for violation in absolute.violations {
        if violation.contains(&golden_prefix) {
            evaluation.warnings.push(violation);
        } else {
            evaluation.violations.push(violation);
        }
    }
    evaluation
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

// ------------------------------------------------------- merge-base goldens

/// The one `git` spawn for this check: runs `args` in `root`, optionally feeding
/// `stdin`, and returns stdout. Direct argv, no shell, waits for exit.
fn git_run(root: &Path, args: &[&str], stdin: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new("git")
        .current_dir(root)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("git {}: {err}", args.join(" ")))?;
    let writer = match (stdin, child.stdin.take()) {
        (Some(input), Some(mut pipe)) => Some(std::thread::spawn(move || pipe.write_all(&input))),
        _ => None,
    };
    let output = child
        .wait_with_output()
        .map_err(|err| format!("git {}: {err}", args.join(" ")))?;
    if let Some(writer) = writer {
        writer
            .join()
            .map_err(|err| format!("git stdin writer panicked: {err:?}"))?
            .map_err(|err| format!("git {} stdin: {err}", args.join(" ")))?;
    }
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn git_text(root: &Path, args: &[&str]) -> Result<String, String> {
    String::from_utf8(git_run(root, args, None)?)
        .map_err(|err| format!("git output is not UTF-8: {err}"))
}

pub(crate) fn is_golden_human_path(path: &str) -> bool {
    path.starts_with("fixtures/") && path.ends_with("/expected/human.txt")
}

/// Parse NUL-separated `ls-tree` records and accept only regular-file goldens.
/// Git symlinks are blobs containing target paths, not the output they point at.
pub(crate) fn golden_paths_from_ls_tree_z(listing: &str) -> Result<Vec<&str>, String> {
    let mut paths = Vec::new();
    for entry in listing.split('\0').filter(|entry| !entry.is_empty()) {
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or("git ls-tree: malformed record")?;
        if !is_golden_human_path(path) {
            continue;
        }
        let mut fields = metadata.split_whitespace();
        let mode = fields.next();
        let kind = fields.next();
        let object = fields.next();
        if !matches!(mode, Some("100644" | "100755"))
            || kind != Some("blob")
            || object.is_none()
            || fields.next().is_some()
        {
            return Err(format!(
                "golden {path} is not a regular Git blob ({metadata})"
            ));
        }
        paths.push(path);
    }
    Ok(paths)
}

/// Scan `git cat-file --batch` output for `paths` (in request order) into
/// counts keyed by `(path, term)`.
pub(crate) fn scan_cat_file_batch(data: &[u8], paths: &[&str]) -> Result<Counts, String> {
    let mut counts = Counts::new();
    let mut pos = 0usize;
    for path in paths {
        let end = data
            .get(pos..)
            .and_then(|rest| rest.iter().position(|byte| *byte == b'\n'))
            .map(|offset| pos + offset)
            .ok_or_else(|| format!("git cat-file: truncated header for {path}"))?;
        let header = String::from_utf8_lossy(&data[pos..end]).to_string();
        let mut parts = header.split(' ');
        let (_, kind, size) = (parts.next(), parts.next(), parts.next());
        if kind != Some("blob") {
            return Err(format!("git cat-file: `{header}` is not a blob"));
        }
        let size: usize = size
            .and_then(|size| size.parse().ok())
            .ok_or_else(|| format!("git cat-file: bad size in `{header}`"))?;
        let start = end + 1;
        let stop = start + size;
        let body = data
            .get(start..stop)
            .ok_or_else(|| format!("git cat-file: truncated body for {path}"))?;
        scan_text(path, path, &String::from_utf8_lossy(body), &mut counts);
        pos = stop + 1;
    }
    Ok(counts)
}

/// The inputs a delta decision was made from, recorded in the report.
pub(crate) struct BaseRead {
    pub upstream: String,
    pub upstream_sha: String,
    pub binding: String,
    pub base_sha: String,
    pub head_sha: String,
    pub dirty_goldens: bool,
    pub counts: Counts,
    pub files: BTreeSet<String>,
    /// Golden counts read from HEAD's blobs (CI only); `None` locally.
    pub head_counts: Option<Counts>,
}

impl BaseRead {
    pub(crate) fn describe(&self) -> String {
        let short = |sha: &str| sha[..sha.len().min(12)].to_string();
        let head_side = if self.head_counts.is_some() {
            "head side is HEAD's committed blobs"
        } else if self.dirty_goldens {
            "head side is the WORKING TREE (uncommitted changes to golden paths)"
        } else {
            "head side is the committed tree"
        };
        format!(
            "Gate: golden-human rows compare per file against the merge-base {} (base {} = {}, {}); HEAD {}; {head_side}; live rows use absolute ceilings.",
            short(&self.base_sha),
            self.upstream,
            short(&self.upstream_sha),
            self.binding,
            short(&self.head_sha),
        )
    }
}

fn resolve_sha(root: &Path, args: &[&str]) -> Result<String, String> {
    let sha = git_text(root, args)?.trim().to_string();
    if sha.len() < 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "git {} did not return a commit sha",
            args.join(" ")
        ));
    }
    Ok(sha)
}

/// Golden counts per `(path, term)` and the set of golden paths in the tree of
/// the commit `sha`, read through Git objects only.
fn golden_counts_at(root: &Path, sha: &str) -> Result<(Counts, BTreeSet<String>), String> {
    let listing = git_text(root, &["ls-tree", "-r", "-z", sha, "--", "fixtures"])?;
    let paths = golden_paths_from_ls_tree_z(&listing)?;
    if paths.is_empty() {
        return Err(format!("no default human.txt goldens found at {sha}"));
    }
    if paths.iter().any(|path| path.contains('\n')) {
        return Err("a golden path contains a newline; cannot batch-read it".to_string());
    }
    let request: String = paths.iter().map(|path| format!("{sha}:{path}\n")).collect();
    let output = git_run(root, &["cat-file", "--batch"], Some(request.into_bytes()))?;
    let files = paths.iter().map(|path| path.to_string()).collect();
    Ok((scan_cat_file_batch(&output, &paths)?, files))
}

/// The GitHub event identity a CI run was started from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CiEvent {
    pub name: String,
    pub payload: String,
    pub checkout: String,
}

/// How the two sides of the comparison are bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// Developer machine: `upstream` ref, working-tree head.
    Local,
    /// Hosted CI: pull_request and merge_group runs bind the base to the event's
    /// immutable sha and verify the checkout; head is HEAD's blobs; a dirty
    /// golden path or an unusable event identity fails closed.
    Ci { event: Result<CiEvent, String> },
}

/// GitHub Actions must identify a real event; other CI environments may not
/// provide GitHub metadata and retain their existing advisory ref fallback.
fn ci_event_name(github: bool, name: Option<String>) -> Result<String, String> {
    let name = name.unwrap_or_default();
    if github
        && (name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
    {
        return Err("GITHUB_EVENT_NAME is missing or invalid under GitHub Actions".into());
    }
    Ok(name)
}

impl Mode {
    pub(crate) fn from_env() -> Mode {
        Mode::from_lookup(&|key| std::env::var(key).ok())
    }

    /// Environment-free core of `from_env`, so every branch is testable.
    pub(crate) fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Mode {
        let github = lookup("GITHUB_ACTIONS").is_some_and(|value| !value.is_empty());
        let ci = github || lookup("CI").is_some_and(|value| value == "true");
        if !ci {
            return Mode::Local;
        }
        let name = match ci_event_name(github, lookup("GITHUB_EVENT_NAME")) {
            Ok(name) => name,
            Err(reason) => return Mode::Ci { event: Err(reason) },
        };
        if name != "pull_request" && name != "merge_group" {
            return Mode::Ci {
                event: Ok(CiEvent {
                    name,
                    payload: String::new(),
                    checkout: String::new(),
                }),
            };
        }
        let event = lookup("GITHUB_EVENT_PATH")
            .ok_or_else(|| "GITHUB_EVENT_PATH is missing".to_string())
            .and_then(|path| {
                fs::read_to_string(path).map_err(|err| format!("read GitHub event payload: {err}"))
            })
            .and_then(|payload| {
                let checkout =
                    lookup("GITHUB_SHA").ok_or_else(|| "GITHUB_SHA is missing".to_string())?;
                Ok(CiEvent {
                    name,
                    payload,
                    checkout,
                })
            });
        Mode::Ci { event }
    }
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Bind a CI comparison to the event's immutable sides. For `pull_request` the
/// checkout must be the integration commit whose parents are exactly the event
/// base and PR head; for `merge_group` HEAD must be the event head and the
/// event base an ancestor. Other events return `None` (the caller uses its
/// ref). Any mismatch, missing identity or dirty golden input is an error.
fn event_base(root: &Path, event: &CiEvent, dirty: bool) -> Result<Option<String>, String> {
    let name = event.name.as_str();
    if name != "pull_request" && name != "merge_group" {
        return Ok(None);
    }
    if dirty {
        return Err("golden working tree is dirty under CI".into());
    }
    let value: serde_json::Value = serde_json::from_str(&event.payload)
        .map_err(|err| format!("invalid GitHub event payload: {err}"))?;
    let sha = |pointer: &str| -> Result<String, String> {
        let found = value
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("missing event identity {pointer}"))?;
        if !is_full_sha(found) {
            return Err(format!("invalid event identity {pointer}"));
        }
        Ok(found.to_ascii_lowercase())
    };
    if !is_full_sha(&event.checkout) {
        return Err("missing or invalid GITHUB_SHA".into());
    }
    let head = resolve_sha(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if head != event.checkout.to_ascii_lowercase() {
        return Err(format!(
            "checkout {head} does not match event checkout {}",
            event.checkout
        ));
    }
    if name == "pull_request" {
        let event_base = sha("/pull_request/base/sha")?;
        let subject = sha("/pull_request/head/sha")?;
        let line = git_text(root, &["rev-list", "--parents", "-n", "1", &head])?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        // The payload's base sha can be older than the base tip GitHub merged
        // into (the base branch moved after the event was created), so the
        // compared base is the integration commit's actual first parent; the
        // event base only has to be that parent or one of its ancestors.
        let [_, first, second] = parts.as_slice() else {
            return Err("PR checkout is not a two-parent integration commit".into());
        };
        if *second != subject {
            return Err("PR checkout's second parent is not the event PR head".into());
        }
        if !is_full_sha(first) {
            return Err("PR checkout has an invalid first parent".into());
        }
        let first = first.to_ascii_lowercase();
        if resolve_sha(root, &["merge-base", &event_base, &first])? != event_base {
            return Err("event base is not the PR checkout's first parent or an ancestor".into());
        }
        Ok(Some(first))
    } else {
        let base = sha("/merge_group/base_sha")?;
        let subject = sha("/merge_group/head_sha")?;
        if head != subject {
            return Err("merge-group checkout does not match event head".into());
        }
        if resolve_sha(root, &["merge-base", &base, &head])? != base {
            return Err("merge-group base is not an ancestor of its head".into());
        }
        Ok(Some(base))
    }
}

/// Banned-term counts per `(golden path, term)` at the merge-base, read through
/// Git objects without building or checking out the base. Locally the base
/// side is the `upstream` ref resolved to a SHA once; under CI pull_request and
/// merge_group runs take it from the verified event. Errors (no upstream,
/// shallow history, unusable CI identity) mean the caller must fall back or
/// fail; this never returns an empty guess.
fn merge_base_golden_counts(root: &Path, upstream: &str, mode: &Mode) -> Result<BaseRead, String> {
    let head_sha = resolve_sha(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let dirty = git_text(
        root,
        &[
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--",
            "fixtures",
        ],
    )?;
    let ignored = git_text(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
            "--",
            "fixtures",
        ],
    )?;
    let dirty_goldens = dirty.split('\0').any(|entry| {
        entry.get(3..).is_some_and(is_golden_human_path) || is_golden_human_path(entry)
    }) || ignored.split('\0').any(is_golden_human_path);
    let bound = match mode {
        Mode::Local => None,
        Mode::Ci { event } => match event {
            Ok(event) => event_base(root, event, dirty_goldens)?,
            Err(reason) => return Err(reason.clone()),
        },
    };
    let (upstream_ref, upstream_sha, binding, base) = match bound {
        Some(base) => (
            "event base".to_string(),
            base.clone(),
            "verified GitHub event identity".to_string(),
            base,
        ),
        None => {
            let sha = resolve_sha(
                root,
                &["rev-parse", "--verify", &format!("{upstream}^{{commit}}")],
            )?;
            let base = resolve_sha(root, &["merge-base", &sha, &head_sha])?;
            (upstream.to_string(), sha, format!("{upstream} ref"), base)
        }
    };
    let (counts, files) = golden_counts_at(root, &base)?;
    let head_counts = match mode {
        Mode::Ci { .. } => Some(golden_counts_at(root, &head_sha)?.0),
        Mode::Local => None,
    };
    Ok(BaseRead {
        upstream: upstream_ref,
        upstream_sha,
        binding,
        base_sha: base,
        head_sha,
        dirty_goldens,
        counts,
        files,
        head_counts,
    })
}

/// Per-`(path, term)` counts of the default goldens under `root`.
pub(crate) fn scan_golden_paths(root: &Path) -> Result<Counts, String> {
    let mut counts = Counts::new();
    for path in golden_human_files(root)? {
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("read golden {}: {err}", path.display()))?;
        let label = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        scan_text(&label, &label, &text, &mut counts);
    }
    Ok(counts)
}

pub(crate) const BASE_UNAVAILABLE_LABEL: &str =
    "base unavailable: absolute-ceiling check only, no PR-relative no-growth established";

const MATCHER_SOURCE: &str = "xtask/src/policy/user_text.rs";

/// The banned needles listed in `user_text.rs` at the commit `sha`. A base with
/// no such file has no matcher to pin, so it pins nothing.
fn base_needles(root: &Path, sha: &str) -> Result<Vec<String>, String> {
    let listing = git_text(root, &["ls-tree", "--name-only", sha, "--", MATCHER_SOURCE])?;
    if listing.trim().is_empty() {
        return Ok(Vec::new());
    }
    let source = git_text(root, &["show", &format!("{sha}:{MATCHER_SOURCE}")])?;
    needles_from_source(&source)
}

/// Pick the gate: per-path delta against the merge-base. Under CI both sides
/// are bound to commits (event base, HEAD blobs) and a dirty golden path or an
/// unreadable/unbound base fails closed. Locally an unreadable base falls back
/// to the absolute ceilings, labelled as not establishing PR-relative
/// no-growth.
pub(crate) fn gate_goldens(
    root: &Path,
    upstream: &str,
    mode: &Mode,
    counts: &Counts,
    head_paths: &Counts,
    baseline: &Baseline,
) -> (Evaluation, String) {
    match merge_base_golden_counts(root, upstream, mode) {
        Ok(read) => {
            let note = read.describe();
            if matches!(mode, Mode::Ci { .. }) && read.dirty_goldens {
                let evaluation = Evaluation {
                    violations: vec![
                        "golden files have uncommitted changes under CI; the head side is not an immutable commit"
                            .to_string(),
                    ],
                    ..Evaluation::default()
                };
                return (evaluation, note);
            }
            let base_sha = read.base_sha.clone();
            let base: BTreeMap<(String, String), usize> = read
                .counts
                .into_iter()
                .map(|(key, found)| (key, found.lines))
                .collect();
            let head = read.head_counts.as_ref().unwrap_or(head_paths);
            let mut evaluation = evaluate_delta(counts, baseline, head, &base, &read.files);
            let mut note = note;
            let in_ci = matches!(mode, Mode::Ci { .. });
            match base_needles(root, &base_sha) {
                Ok(needles) => {
                    for lost in lost_needles(&needles) {
                        evaluation.violations.push(format!(
                            "banned needle `{lost}` at the merge-base is no longer detected by this matcher; the gate cannot qualify its own weakening"
                        ));
                    }
                }
                Err(reason) if in_ci => evaluation.violations.push(format!(
                    "could not read the merge-base banned list to check the matcher ({reason})"
                )),
                Err(reason) => {
                    note.push_str(&format!(" Matcher pin not checked locally ({reason})."))
                }
            }
            // A push or scheduled run has no PR delta (HEAD is the base), so the
            // per-file golden ceilings are its only backstop: a breach there
            // means the PR gate was bypassed or weakened, and fails the run.
            if let Mode::Ci { event: Ok(event) } = mode
                && event.name != "pull_request"
                && event.name != "merge_group"
            {
                let golden_prefix = format!("in {GOLDEN_SURFACE}:");
                let (golden, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut evaluation.warnings)
                    .into_iter()
                    .partition(|warning| warning.contains(&golden_prefix));
                evaluation.warnings = rest;
                evaluation.violations.extend(golden);
            }
            (evaluation, note)
        }
        Err(reason) => {
            let fetch = "Run `git fetch --deepen=200 origin main` (or `git fetch origin main`) so the per-PR delta gate can run.";
            if matches!(mode, Mode::Ci { .. }) {
                let note = format!(
                    "Gate: {BASE_UNAVAILABLE_LABEL} ({reason}). Failing closed under CI. {fetch}"
                );
                let evaluation = Evaluation {
                    violations: vec![format!(
                        "merge-base unreadable under CI ({reason}); the per-PR delta cannot be established"
                    )],
                    ..Evaluation::default()
                };
                (evaluation, note)
            } else {
                (
                    evaluate(counts, baseline),
                    format!(
                        "Gate: {BASE_UNAVAILABLE_LABEL} ({reason}); falling back to absolute ceilings for every row. {fetch}"
                    ),
                )
            }
        }
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
    let mut full = vec![
        "-c",
        "user.email=user-text@example.invalid",
        "-c",
        "user.name=user-text",
        "-c",
        "commit.gpgsign=false",
    ];
    full.extend_from_slice(args);
    git_run(repo, &full, None).map(|_| ())
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
        return Ok(path);
    }
    // The `cargo xtask` alias runs the driver from `target/xtask-driver`
    // while `cargo build -p ripr` writes to the shared workspace target
    // directory, so look there when the driver directory has no product binary.
    let profile = if release { "release" } else { "debug" };
    let shared = Path::new("target").join(profile).join(name);
    if shared.is_file() {
        shared
            .canonicalize()
            .map_err(|err| format!("resolve {}: {err}", shared.display()))
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
    gate_note: &str,
) -> String {
    let mut out = format!("# check-user-text report\n\nStatus: {status}\n\n{gate_note}\n\n");
    if !evaluation.warnings.is_empty() {
        out.push_str(&format!(
            "WARNING (main health, does not fail this PR): {} golden count(s) are above their absolute ceiling. This tree did not add them relative to the merge-base; lower them with wording work, then `--ratchet`:\n",
            evaluation.warnings.len()
        ));
        for item in &evaluation.warnings {
            out.push_str(&format!("- {item}\n"));
        }
        out.push('\n');
    }
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
        out.push_str("\nRose (new banned-term lines or live ceiling):\n");
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

/// `--tree <path>` (#11385): scan and gate the goldens of another checkout with
/// the matcher compiled into this binary. CI builds the merge-base's xtask in a
/// separate worktree and points it at the PR tree, so the judge is the code the
/// PR could not have changed. `.github/workflows/rust-gates.yml` looks for this literal in the
/// base source to know whether the merge-base can do that yet.
const TREE_FLAG: &str = "--tree";

fn tree_root(path: &Path) -> Result<PathBuf, String> {
    let root = path
        .canonicalize()
        .map_err(|err| format!("{TREE_FLAG} {}: {err}", path.display()))?;
    if !root.join(BASELINE_PATH).is_file() {
        return Err(format!(
            "{TREE_FLAG} {} is not a ripr checkout (no {BASELINE_PATH})",
            root.display()
        ));
    }
    Ok(root)
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
    let mut tree: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            TREE_FLAG => {
                tree = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| format!("{TREE_FLAG} needs a path"))?,
                ));
            }
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
                    "unknown argument `{other}`; usage: cargo xtask check-user-text [--ripr <path>] [--ratchet] [--full] [--tree <path>]"
                ));
            }
        }
    }
    if tree.is_some() && (ratchet || full || ripr.is_some()) {
        return Err(format!(
            "{TREE_FLAG} judges committed goldens only; it cannot combine with --ratchet, --full or --ripr"
        ));
    }
    let root = match &tree {
        Some(path) => tree_root(path)?,
        None => repo_root()?,
    };
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
    // A foreign tree has no binary built from it, so its live output is not
    // judged here; the golden delta is the whole judgment in that mode.
    let run_live =
        tree.is_none() && live_half_decision(full || ratchet || ripr.is_some(), changed_paths());
    let (live, baseline) = if run_live {
        let bin = resolve_ripr_binary(ripr.as_deref())?;
        (scan_live(&bin, &root, &mut counts)?, baseline)
    } else {
        if tree.is_some() {
            println!(
                "check-user-text: live output half skipped, --tree judges the goldens of {} with this binary's matcher",
                root.display()
            );
        } else {
            println!(
                "check-user-text: live output half skipped, no ripr source or baseline change vs origin/main (golden half only; use --full to force)"
            );
        }
        let golden_only = baseline
            .into_iter()
            .filter(|((surface, _), _)| surface == GOLDEN_SURFACE)
            .collect();
        (Vec::new(), golden_only)
    };

    let scanned = Scanned {
        counts,
        baseline,
        live,
        golden_files,
    };
    finish_check(&root, "origin/main", &Mode::from_env(), &scanned, ratchet)
}

/// What the scan half produced, handed to the gate.
struct Scanned {
    counts: Counts,
    baseline: Baseline,
    live: Vec<String>,
    golden_files: usize,
}

/// Gate the scanned counts, write the report and decide the result. The command
/// calls this with `origin/main` and the real CI flag; tests drive it directly
/// against temp repositories.
fn finish_check(
    root: &Path,
    upstream: &str,
    mode: &Mode,
    scanned: &Scanned,
    ratchet: bool,
) -> Result<(), String> {
    let Scanned {
        counts,
        baseline,
        live,
        golden_files,
    } = scanned;
    // Golden rows gate per file on this tree's delta (#11374).
    let head_paths = scan_golden_paths(root)?;
    let (evaluation, gate_note) = gate_goldens(root, upstream, mode, counts, &head_paths, baseline);
    println!("check-user-text: {gate_note}");
    let status = if evaluation.violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let report = render_report(
        status,
        *golden_files,
        live,
        counts,
        baseline,
        &evaluation,
        &gate_note,
    );
    write_report(root, &report);
    print!("{report}");
    if ratchet {
        if let Some(lowered) = ratchet_outcome(counts, baseline) {
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
            "{} plain-language term count(s) rose (delta or ceiling); see {REPORT_PATH} and {GLOSSARY}",
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

    /// Per-path counts for `(path, text)` goldens, and their surface totals.
    fn goldens(files: &[(&str, &str)]) -> (Counts, Counts) {
        let mut paths = Counts::new();
        let mut totals = Counts::new();
        for (path, text) in files {
            scan_text(path, path, text, &mut paths);
            scan_text(GOLDEN_SURFACE, path, text, &mut totals);
        }
        (paths, totals)
    }

    fn base_of(files: &[(&str, &str)]) -> BTreeMap<(String, String), usize> {
        goldens(files)
            .0
            .iter()
            .map(|(key, found)| (key.clone(), found.lines))
            .collect()
    }

    fn delta(base: &[(&str, &str)], head: &[(&str, &str)], baseline: &Baseline) -> Evaluation {
        let (paths, totals) = goldens(head);
        let files = base.iter().map(|(path, _)| path.to_string()).collect();
        evaluate_delta(&totals, baseline, &paths, &base_of(base), &files)
    }

    const A: &str = "fixtures/a/expected/human.txt";
    const B: &str = "fixtures/b/expected/human.txt";

    /// #11374 positive: adding a banned golden line fails against the merge-base
    /// even when the absolute ceiling would have allowed it.
    #[test]
    fn delta_gate_fails_a_pr_that_adds_a_banned_line() {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 100);
        let evaluation = delta(
            &[(A, "a seam\nb seam\n")],
            &[(A, "a seam\nb seam\nc seam\n")],
            &baseline,
        );
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
        assert!(evaluation.violations[0].contains("+1"));
        assert!(evaluation.violations[0].contains(A));
    }

    /// A new term with no base row and no ceiling also fails (any row).
    #[test]
    fn delta_gate_fails_a_newly_introduced_term() {
        let evaluation = delta(
            &[(A, "plain\n")],
            &[(A, "1 probe checked\n")],
            &Baseline::new(),
        );
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
    }

    /// Negative: removing banned lines passes with no baseline edit.
    #[test]
    fn delta_gate_passes_a_pr_that_removes_banned_lines() {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 2);
        let evaluation = delta(&[(A, "a seam\nb seam\n")], &[(A, "a seam\n")], &baseline);
        assert!(evaluation.violations.is_empty(), "{evaluation:?}");
    }

    /// Netting: deleting banned lines in one golden buys no room for new jargon
    /// in another golden, and a path new at the head counts in full.
    #[test]
    fn delta_gate_does_not_net_removals_against_additions_across_paths() {
        let base = [(A, "a seam\nb seam\n"), (B, "plain\n")];
        let moved = delta(
            &base,
            &[(A, "a seam\n"), (B, "now a seam\n")],
            &Baseline::new(),
        );
        assert_eq!(moved.violations.len(), 1, "{moved:?}");
        assert!(moved.violations[0].contains(B), "{moved:?}");

        let fresh = delta(
            &base,
            &[
                (A, "plain\n"),
                (B, "plain\n"),
                ("fixtures/c/expected/human.txt", "a seam\n"),
            ],
            &Baseline::new(),
        );
        assert_eq!(fresh.violations.len(), 1, "{fresh:?}");
        assert!(fresh.violations[0].contains("new path"), "{fresh:?}");
    }

    /// Documented false red: a rename that keeps old jargon reads as a new path.
    #[test]
    fn delta_gate_treats_a_renamed_golden_as_a_new_path() {
        let evaluation = delta(&[(A, "a seam\n")], &[(B, "a seam\n")], &Baseline::new());
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
    }

    /// Negative: an unrelated PR passes while main is above its absolute ceiling,
    /// and the over-ceiling state stays visible as a warning.
    #[test]
    fn delta_gate_passes_unrelated_pr_while_over_absolute_ceiling() {
        let mut baseline = Baseline::new();
        baseline.insert((GOLDEN_SURFACE.into(), "seam".into()), 1);
        let same = [(A, "a seam\nb seam\n")];
        let evaluation = delta(&same, &same, &baseline);
        assert!(evaluation.violations.is_empty(), "{evaluation:?}");
        assert_eq!(evaluation.warnings.len(), 1, "{evaluation:?}");
        // The absolute fallback (shallow clone) still fails closed on the same input.
        assert_eq!(evaluate(&goldens(&same).1, &baseline).violations.len(), 1);
    }

    /// Live surfaces keep absolute ceilings: a rise there fails regardless of base.
    #[test]
    fn delta_gate_keeps_absolute_ceilings_for_live_surfaces() {
        let live = scan("check", "a discriminator\n");
        let evaluation = evaluate_delta(
            &live,
            &Baseline::new(),
            &Counts::new(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
        assert!(evaluation.warnings.is_empty());
    }

    #[test]
    fn golden_path_filter_matches_only_default_human_goldens() {
        assert!(is_golden_human_path("fixtures/a/expected/human.txt"));
        assert!(!is_golden_human_path("fixtures/a/expected/human-full.txt"));
        assert!(!is_golden_human_path("docs/expected/human.txt"));
    }

    #[test]
    fn ls_tree_z_listing_keeps_quoted_and_non_ascii_golden_paths() -> Result<(), String> {
        let listing = "100644 blob abc\tfixtures/a/expected/human.txt\x00100755 blob def\tfixtures/caf\u{e9} \"q\"/expected/human.txt\x00120000 blob ghi\tfixtures/a/expected/human-full.txt\x00100644 blob jkl\tfixtures/a/input.rs\0";
        assert_eq!(
            golden_paths_from_ls_tree_z(listing)?,
            vec![
                "fixtures/a/expected/human.txt",
                "fixtures/caf\u{e9} \"q\"/expected/human.txt"
            ]
        );
        Ok(())
    }

    #[test]
    fn ls_tree_rejects_nonregular_selected_goldens() {
        assert!(
            matches!(golden_paths_from_ls_tree_z("120000 blob abc\tfixtures/a/expected/human.txt\0"), Err(ref e) if e.contains("not a regular Git blob"))
        );
        assert!(
            matches!(golden_paths_from_ls_tree_z("160000 commit abc\tfixtures/a/expected/human.txt\0"), Err(ref e) if e.contains("not a regular Git blob"))
        );
    }

    #[test]
    fn cat_file_batch_output_is_scanned_per_path() -> Result<(), String> {
        let first = "one seam\ntwo seam\n";
        let second = "clean\n";
        let data = format!(
            "{} blob {}\n{first}\n{} blob {}\n{second}\n",
            "a".repeat(40),
            first.len(),
            "b".repeat(40),
            second.len()
        );
        let counts = scan_cat_file_batch(data.as_bytes(), &[A, B])?;
        let seams = counts.get(&(A.to_string(), "seam".to_string()));
        assert_eq!(seams.map(|found| found.lines), Some(2));
        assert!(!counts.keys().any(|(path, _)| path == B));

        let missing = format!("{}:{A} missing\n", "a".repeat(40));
        assert!(
            matches!(scan_cat_file_batch(missing.as_bytes(), &[A]), Err(ref e) if e.contains("not a blob"))
        );
        assert!(
            matches!(scan_cat_file_batch(b"", &[A]), Err(ref e) if e.contains("truncated header"))
        );
        let truncated = format!("{} blob 99\nshort\n", "a".repeat(40));
        assert!(
            matches!(scan_cat_file_batch(truncated.as_bytes(), &[A]), Err(ref e) if e.contains("truncated body"))
        );
        Ok(())
    }

    /// A temp repository with `main` holding `base` goldens and `HEAD` on a
    /// branch holding `head` goldens. Returns the root.
    fn golden_repo(
        name: &str,
        base: &[(&str, &str)],
        head: &[(&str, &str)],
    ) -> Result<PathBuf, String> {
        let root = temp_root(name)?;
        run_git(&root, &["init", "-q", "-b", "main"])?;
        run_git(&root, &["config", "user.email", "t@example.com"])?;
        run_git(&root, &["config", "user.name", "t"])?;
        run_git(&root, &["config", "commit.gpgsign", "false"])?;
        for (path, text) in base {
            write_file(&root.join(path), text)?;
        }
        run_git(&root, &["add", "-A"])?;
        run_git(&root, &["commit", "-q", "-m", "base"])?;
        run_git(&root, &["checkout", "-q", "-b", "pr"])?;
        for (path, text) in head {
            write_file(&root.join(path), text)?;
        }
        run_git(&root, &["add", "-A"])?;
        run_git(&root, &["commit", "-q", "--allow-empty", "-m", "head"])?;
        Ok(root)
    }

    fn gate_in(root: &Path, upstream: &str) -> Result<(Evaluation, String), String> {
        gate_in_ci(root, upstream, &Mode::Local)
    }

    fn gate_in_ci(
        root: &Path,
        upstream: &str,
        mode: &Mode,
    ) -> Result<(Evaluation, String), String> {
        let mut totals = Counts::new();
        scan_goldens(root, &mut totals)?;
        let paths = scan_golden_paths(root)?;
        Ok(gate_goldens(
            root,
            upstream,
            mode,
            &totals,
            &paths,
            &Baseline::new(),
        ))
    }

    /// The production gate function applies the merge-base delta end to end:
    /// an added banned golden line fails, and removing it passes.
    #[test]
    fn gate_goldens_reads_the_merge_base_and_applies_the_delta() -> Result<(), String> {
        let base = [(A, "a seam\n"), (B, "plain\n")];
        let added = golden_repo(
            "gate-add",
            &base,
            &[(A, "a seam\n"), (B, "plain\nnew seam here\n")],
        )?;
        let (failing, note) = gate_in(&added, "main")?;
        let _ = fs::remove_dir_all(&added);
        assert_eq!(failing.violations.len(), 1, "{failing:?}");
        assert!(
            note.contains("merge-base") && !note.contains("unavailable"),
            "{note}"
        );

        let removed = golden_repo("gate-remove", &base, &[(A, "plain\n"), (B, "plain\n")])?;
        let (passing, _) = gate_in(&removed, "main")?;
        let _ = fs::remove_dir_all(&removed);
        assert!(passing.violations.is_empty(), "{passing:?}");
        Ok(())
    }

    #[test]
    fn tree_must_be_a_ripr_checkout_and_names_a_path() -> Result<(), String> {
        let empty =
            std::env::temp_dir().join(format!("ripr-user-text-tree-{}", std::process::id()));
        fs::create_dir_all(&empty).map_err(|err| err.to_string())?;
        let outcome = tree_root(&empty);
        let _ = fs::remove_dir_all(&empty);
        assert!(
            matches!(outcome, Err(ref e) if e.contains("not a ripr checkout")),
            "{outcome:?}"
        );
        let missing = tree_root(&empty);
        assert!(
            matches!(missing, Err(ref e) if e.contains(TREE_FLAG)),
            "{missing:?}"
        );
        let no_path = check_user_text_with_args(&[TREE_FLAG.to_string()]);
        assert!(
            matches!(no_path, Err(ref e) if e.contains("needs a path")),
            "{no_path:?}"
        );
        Ok(())
    }

    #[test]
    fn tree_judges_goldens_only_and_rejects_the_modes_that_need_a_local_binary() {
        for other in ["--ratchet", "--full"] {
            let outcome =
                check_user_text_with_args(&[TREE_FLAG.to_string(), ".".into(), other.to_string()]);
            assert!(
                matches!(outcome, Err(ref e) if e.contains("goldens only")),
                "{other}: {outcome:?}"
            );
        }
    }

    fn scanned_in(root: &Path) -> Result<Scanned, String> {
        let mut counts = Counts::new();
        let golden_files = scan_goldens(root, &mut counts)?;
        Ok(Scanned {
            counts,
            baseline: Baseline::new(),
            live: Vec::new(),
            golden_files,
        })
    }

    /// B1: the function the command ends in applies the merge-base delta, the
    /// CI flag and the upstream it is given, end to end through Git.
    #[test]
    fn finish_check_applies_the_delta_and_the_ci_flag() -> Result<(), String> {
        let base = [(A, "a seam\n")];
        let grown = golden_repo("finish-add", &base, &[(A, "a seam\nanother seam\n")])?;
        let failed = finish_check(&grown, "main", &Mode::Local, &scanned_in(&grown)?, false);
        let _ = fs::remove_dir_all(&grown);
        let message = failed
            .err()
            .ok_or("an added golden banned line must fail")?;
        assert!(message.contains("rose"), "{message}");

        let shrunk = golden_repo("finish-remove", &base, &[(A, "plain\n")])?;
        let passed = finish_check(&shrunk, "main", &Mode::Local, &scanned_in(&shrunk)?, false);
        let _ = fs::remove_dir_all(&shrunk);
        assert_eq!(passed, Ok(()));

        let no_upstream = golden_repo("finish-ci", &base, &base)?;
        let local = finish_check(
            &no_upstream,
            "origin/main",
            &Mode::Local,
            &scanned_in(&no_upstream)?,
            false,
        );
        let in_ci = finish_check(
            &no_upstream,
            "origin/main",
            &ci_other(),
            &scanned_in(&no_upstream)?,
            false,
        );
        let _ = fs::remove_dir_all(&no_upstream);
        // Absolute fallback: a zero baseline makes the existing seam line fail
        // locally too, so assert the CI result is a failure and local differs
        // only by note; the CI-only difference is shown in the next test.
        assert!(in_ci.is_err(), "{in_ci:?}");
        assert!(local.is_err(), "{local:?}");
        Ok(())
    }

    /// With nothing banned, ci=false falls back and passes while ci=true fails
    /// closed: the flag is what changes the command's result.
    #[test]
    fn finish_check_fails_closed_only_under_ci_without_a_base() -> Result<(), String> {
        let root = golden_repo("finish-clean", &[(A, "plain\n")], &[(A, "plain\n")])?;
        let local = finish_check(
            &root,
            "origin/main",
            &Mode::Local,
            &scanned_in(&root)?,
            false,
        );
        let in_ci = finish_check(
            &root,
            "origin/main",
            &ci_other(),
            &scanned_in(&root)?,
            false,
        );
        let _ = fs::remove_dir_all(&root);
        assert_eq!(local, Ok(()));
        assert!(in_ci.is_err(), "{in_ci:?}");
        Ok(())
    }

    /// The command itself must hand the real upstream and CI flag to the gate
    /// and must not gate goldens with the absolute `evaluate` directly.
    #[test]
    fn the_command_calls_the_gate_with_origin_main_and_the_ci_flag() -> Result<(), String> {
        let source = fs::read_to_string(repo_root()?.join("xtask/src/policy/user_text.rs"))
            .map_err(|err| format!("read user_text.rs: {err}"))?;
        let start = source
            .find("pub(crate) fn check_user_text_with_args(")
            .ok_or("command not found")?;
        let end = source[start..]
            .find("\n}\n")
            .ok_or("command end not found")?
            + start;
        let command = &source[start..end];
        assert!(
            command.contains(
                "finish_check(&root, \"origin/main\", &Mode::from_env(), &scanned, ratchet)"
            ),
            "command must call finish_check with origin/main and running_in_ci()"
        );
        assert!(
            !command.contains("evaluate("),
            "command must not gate with evaluate("
        );
        let fin = source
            .find("\nfn finish_check(")
            .ok_or("finish_check not found")?;
        let fin_end = source[fin..].find("\n}\n").ok_or("finish_check end")? + fin;
        let fin_body = &source[fin..fin_end];
        assert!(fin_body.contains("gate_goldens(root, upstream, mode,"));
        assert!(!fin_body.contains("evaluate("));
        Ok(())
    }

    #[test]
    fn github_event_name_requires_nonempty_valid_name() {
        assert!(
            matches!(ci_event_name(true, None), Err(ref e) if e.contains("missing or invalid"))
        );
        assert!(
            matches!(ci_event_name(true, Some(String::new())), Err(ref e) if e.contains("missing or invalid"))
        );
        assert!(
            matches!(ci_event_name(true, Some("  ".into())), Err(ref e) if e.contains("missing or invalid"))
        );
        assert!(
            matches!(ci_event_name(true, Some("pull-request".into())), Err(ref e) if e.contains("missing or invalid"))
        );
        assert_eq!(ci_event_name(true, Some("push".into())), Ok("push".into()));
        assert_eq!(
            ci_event_name(true, Some("workflow_dispatch".into())),
            Ok("workflow_dispatch".into())
        );
        assert_eq!(ci_event_name(false, None), Ok(String::new()));
    }

    /// CI run with no pull_request/merge_group identity (for example `push`).
    fn ci_other() -> Mode {
        Mode::Ci {
            event: Ok(CiEvent {
                name: "push".into(),
                payload: String::new(),
                checkout: String::new(),
            }),
        }
    }

    /// A pull_request checkout: `main` holds `base`, `pr` holds `head`, HEAD is
    /// their merge commit (detached), and the returned mode carries the event.
    fn pr_event_repo(
        name: &str,
        base: &[(&str, &str)],
        head: &[(&str, &str)],
    ) -> Result<(PathBuf, String, String, String), String> {
        let root = golden_repo(name, base, head)?;
        let pr = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        let main = git_text(&root, &["rev-parse", "main"])?.trim().to_string();
        run_git(&root, &["checkout", "-q", "main"])?;
        run_git(
            &root,
            &["merge", "-q", "--no-ff", "pr", "-m", "integration"],
        )?;
        let merge = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        run_git(&root, &["checkout", "-q", "--detach"])?;
        Ok((root, main, pr, merge))
    }

    fn pr_mode(base: &str, head: &str, checkout: &str) -> Mode {
        Mode::Ci {
            event: Ok(CiEvent {
                name: "pull_request".into(),
                payload: serde_json::json!({
                    "pull_request": {"base": {"sha": base}, "head": {"sha": head}}
                })
                .to_string(),
                checkout: checkout.into(),
            }),
        }
    }

    /// Every needle the head lists must itself stay detectable in the probe
    /// contexts, or the base-vs-head pin would false-red on an untouched tree.
    #[test]
    fn the_matcher_detects_every_needle_it_lists_in_every_probe_context() -> Result<(), String> {
        let own = needles_from_source(include_str!("user_text.rs"))?;
        let listed: Vec<String> = BANNED
            .iter()
            .flat_map(|banned| banned.needles.iter().map(|n| (*n).to_string()))
            .collect();
        assert_eq!(
            own, listed,
            "the text reader must see exactly the BANNED needles"
        );
        assert!(
            lost_needles(&own).is_empty(),
            "lost: {:?}",
            lost_needles(&own)
        );
        Ok(())
    }

    /// Negative: a needle the base banned but the head does not list is lost.
    #[test]
    fn a_needle_dropped_from_the_head_list_is_reported_lost() {
        let base = vec!["seam".to_string(), "frobnicate".to_string()];
        assert_eq!(lost_needles(&base), vec!["frobnicate".to_string()]);
        assert!(lost_needles(&["ripr-spec-".to_string()]).is_empty());
    }

    #[test]
    fn the_text_reader_fails_closed_on_a_malformed_table() {
        assert!(
            matches!(needles_from_source("fn main() {}"), Err(ref e) if e.contains("no `const BANNED`"))
        );
        assert!(matches!(
            needles_from_source("const BANNED: &[Banned] = &[\n];"),
            Err(ref e) if e.contains("no needles")
        ));
        assert!(matches!(
            needles_from_source("const BANNED: &[Banned] = &[ needles: &[\"a\\\\b\"] \n];"),
            Err(ref e) if e.contains("escaped needle")
        ));
        assert_eq!(
            needles_from_source("const BANNED: &[Banned] = &[\n needles: &[\"x\", \"y\"],\n];"),
            Ok(vec!["x".to_string(), "y".to_string()])
        );
    }

    fn matcher_source(needles: &str) -> String {
        format!(
            "const BANNED: &[Banned] = &[\n    Banned {{ id: \"t\", needles: &[{needles}] }},\n];\n"
        )
    }

    /// A PR that drops a term the base banned fails under CI even though both
    /// sides of the golden delta are clean, and a base without the file pins nothing.
    #[test]
    fn ci_fails_a_pr_whose_matcher_lost_a_base_needle() -> Result<(), String> {
        let (root, main, pr, merge) = pr_event_repo(
            "pin-lost",
            &[
                (A, "plain\n"),
                (MATCHER_SOURCE, &matcher_source("\"seam\", \"frobnicate\"")),
            ],
            &[(A, "plain\n")],
        )?;
        let mode = pr_mode(&main, &pr, &merge);
        let (evaluation, _) = gate_in_ci(&root, "main", &mode)?;
        let _ = fs::remove_dir_all(&root);
        assert!(
            evaluation
                .violations
                .iter()
                .any(|v| v.contains("`frobnicate`")),
            "{:?}",
            evaluation.violations
        );
        assert!(!evaluation.violations.iter().any(|v| v.contains("`seam`")));
        Ok(())
    }

    #[test]
    fn ci_with_an_unreadable_base_list_fails_and_a_missing_file_pins_nothing() -> Result<(), String>
    {
        let (bad, main, pr, merge) = pr_event_repo(
            "pin-bad",
            &[(A, "plain\n"), (MATCHER_SOURCE, "no table here\n")],
            &[(A, "plain\n")],
        )?;
        let (evaluation, _) = gate_in_ci(&bad, "main", &pr_mode(&main, &pr, &merge))?;
        let _ = fs::remove_dir_all(&bad);
        assert!(
            evaluation
                .violations
                .iter()
                .any(|v| v.contains("could not read the merge-base banned list")),
            "{:?}",
            evaluation.violations
        );
        let (none, main, pr, merge) =
            pr_event_repo("pin-none", &[(A, "plain\n")], &[(A, "plain\n")])?;
        let (evaluation, _) = gate_in_ci(&none, "main", &pr_mode(&main, &pr, &merge))?;
        let _ = fs::remove_dir_all(&none);
        assert!(
            evaluation.violations.is_empty(),
            "{:?}",
            evaluation.violations
        );
        Ok(())
    }

    /// A push has no PR delta, so a golden over its per-file ceiling fails there
    /// (the backstop) while the same state is only a warning on a PR.
    #[test]
    fn a_push_run_fails_a_golden_over_its_ceiling_but_a_pr_run_only_warns() -> Result<(), String> {
        let root = golden_repo("push-backstop", &[(A, "one seam\n")], &[(A, "one seam\n")])?;
        let mut counts = Counts::new();
        scan_goldens(&root, &mut counts)?;
        let head_paths = scan_golden_paths(&root)?;
        let baseline = Baseline::new();
        let (push, _) = gate_goldens(&root, "main", &ci_other(), &counts, &head_paths, &baseline);
        let (local, _) = gate_goldens(&root, "main", &Mode::Local, &counts, &head_paths, &baseline);
        let _ = fs::remove_dir_all(&root);
        assert!(
            push.violations
                .iter()
                .any(|v| v.contains("in golden-human:")),
            "{:?}",
            push.violations
        );
        assert!(local.violations.is_empty(), "{:?}", local.violations);
        assert!(
            local
                .warnings
                .iter()
                .any(|v| v.contains("in golden-human:"))
        );
        Ok(())
    }

    /// A committed symlink is not golden output: its blob is only a target path.
    #[cfg(unix)]
    #[test]
    fn ci_rejects_a_committed_symlink_golden() -> Result<(), String> {
        use std::os::unix::fs::symlink;
        let root = golden_repo("ci-symlink", &[(A, "plain\n")], &[(A, "plain\n")])?;
        let base = git_text(&root, &["rev-parse", "main"])?.trim().to_string();
        fs::remove_file(root.join(A)).map_err(|e| e.to_string())?;
        write_file(&root.join("fixtures/a/target.txt"), "added seam\n")?;
        symlink("../target.txt", root.join(A)).map_err(|e| e.to_string())?;
        run_git(&root, &["add", "-A"])?;
        run_git(&root, &["commit", "-q", "-m", "symlink golden"])?;
        let pr = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        run_git(&root, &["checkout", "-q", "main"])?;
        run_git(
            &root,
            &["merge", "-q", "--no-ff", "pr", "-m", "integration"],
        )?;
        let merge = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        let mode = pr_mode(&base, &pr, &merge);
        let result = merge_base_golden_counts(&root, "main", &mode);
        assert!(
            matches!(result, Err(ref reason) if reason.contains("not a regular Git blob")),
            "a committed symlink must not be scanned as its target-path blob"
        );
        let (evaluation, _) = gate_goldens(
            &root,
            "main",
            &mode,
            &Counts::new(),
            &Counts::new(),
            &Baseline::new(),
        );
        assert!(!evaluation.violations.is_empty(), "CI must fail closed");
        let mut counts = Counts::new();
        assert!(
            matches!(scan_goldens(&root, &mut counts), Err(ref e) if e.contains("not a regular file")),
            "local scans must reject selected symlinks too"
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    /// Negative: a ref moved onto the candidate hides growth locally, but under
    /// CI the base is the event sha, so the same input still fails.
    #[test]
    fn ci_ignores_a_base_ref_pointed_at_the_candidate() -> Result<(), String> {
        let (root, base, pr, merge) =
            pr_event_repo("ci-ref", &[(A, "a seam\n")], &[(A, "a seam\nmore seam\n")])?;
        run_git(&root, &["branch", "-f", "main", "HEAD"])?;
        let (local, _) = gate_in_ci(&root, "main", &Mode::Local)?;
        let (ci, note) = gate_in_ci(&root, "main", &pr_mode(&base, &pr, &merge))?;
        let _ = fs::remove_dir_all(&root);
        assert!(
            local.violations.is_empty(),
            "ref lookup is fooled: {local:?}"
        );
        assert_eq!(ci.violations.len(), 1, "{ci:?}");
        assert!(note.contains("HEAD's committed blobs"), "{note}");
        assert!(note.contains("verified GitHub event identity"), "{note}");
        Ok(())
    }

    /// Negative: under CI an edited working-tree golden fails closed, even when
    /// the edit hides the committed growth.
    #[test]
    fn ci_fails_closed_on_a_dirty_golden_path() -> Result<(), String> {
        let (root, base, pr, merge) = pr_event_repo(
            "ci-dirty",
            &[(A, "a seam\n")],
            &[(A, "a seam\nmore seam\n")],
        )?;
        write_file(&root.join(A), "a seam\n")?;
        let (ci, _) = gate_in_ci(&root, "main", &pr_mode(&base, &pr, &merge))?;
        let _ = fs::remove_dir_all(&root);
        assert!(!ci.violations.is_empty(), "{ci:?}");
        Ok(())
    }

    /// Negative: mismatched event identities (wrong base, wrong PR head, wrong
    /// checkout) fail closed rather than falling back to a ref.
    #[test]
    fn ci_rejects_mismatched_event_identities() -> Result<(), String> {
        let (root, base, pr, merge) =
            pr_event_repo("ci-ident", &[(A, "a seam\n")], &[(A, "a seam\n")])?;
        let good = gate_in_ci(&root, "main", &pr_mode(&base, &pr, &merge))?.0;
        let wrong_base = gate_in_ci(&root, "main", &pr_mode(&pr, &pr, &merge))?;
        let wrong_head = gate_in_ci(&root, "main", &pr_mode(&base, &base, &merge))?;
        let wrong_checkout = gate_in_ci(&root, "main", &pr_mode(&base, &pr, &base))?;
        let missing = gate_in_ci(
            &root,
            "main",
            &Mode::Ci {
                event: Err("GITHUB_EVENT_NAME is missing or invalid under GitHub Actions".into()),
            },
        )?;
        let _ = fs::remove_dir_all(&root);
        assert!(good.violations.is_empty(), "{good:?}");
        for (label, (evaluation, note), reason) in [
            ("wrong base", wrong_base, "first parent or an ancestor"),
            (
                "wrong head",
                wrong_head,
                "second parent is not the event PR head",
            ),
            (
                "wrong checkout",
                wrong_checkout,
                "does not match event checkout",
            ),
            ("missing event", missing, "GITHUB_EVENT_NAME is missing"),
        ] {
            assert_eq!(evaluation.violations.len(), 1, "{label}: {evaluation:?}");
            assert!(note.contains(reason), "{label}: {note}");
        }
        Ok(())
    }

    /// The payload base can be older than the base tip GitHub merged into; the
    /// compared base is then the checkout's first parent, so main's own movement
    /// is not charged to the PR.
    #[test]
    fn ci_pull_request_accepts_a_stale_event_base_and_compares_to_the_first_parent()
    -> Result<(), String> {
        let root = golden_repo("ci-stale", &[(A, "plain\n")], &[(A, "plain\n")])?;
        let pr = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        let old_base = git_text(&root, &["rev-parse", "main"])?.trim().to_string();
        run_git(&root, &["checkout", "-q", "main"])?;
        write_file(&root.join(B), "a seam\n")?;
        run_git(&root, &["add", "-A"])?;
        run_git(
            &root,
            &["commit", "-q", "-m", "main moved: new banned golden"],
        )?;
        run_git(
            &root,
            &["merge", "-q", "--no-ff", "pr", "-m", "integration"],
        )?;
        let merge = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        run_git(&root, &["checkout", "-q", "--detach"])?;
        let (stale, note) = gate_in_ci(&root, "main", &pr_mode(&old_base, &pr, &merge))?;
        let _ = fs::remove_dir_all(&root);
        assert!(stale.violations.is_empty(), "{stale:?} {note}");
        assert!(note.contains("verified GitHub event identity"), "{note}");
        Ok(())
    }

    fn mg_mode(base: &str, head: &str, checkout: &str) -> Mode {
        Mode::Ci {
            event: Ok(CiEvent {
                name: "merge_group".into(),
                payload: serde_json::json!({
                    "merge_group": {"base_sha": base, "head_sha": head}
                })
                .to_string(),
                checkout: checkout.into(),
            }),
        }
    }

    /// merge_group binding: HEAD is the event head and the event base is its
    /// ancestor; growth against that base fails, wrong identities fail closed.
    #[test]
    fn ci_merge_group_binds_head_and_base_ancestry() -> Result<(), String> {
        let (root, base, pr, _) =
            pr_event_repo("ci-mg", &[(A, "a seam\n")], &[(A, "a seam\nmore seam\n")])?;
        let head = pr.clone();
        run_git(&root, &["checkout", "-q", "--detach", &head])?;
        let good = gate_in_ci(&root, "main", &mg_mode(&base, &head, &head))?;
        let wrong_head = gate_in_ci(&root, "main", &mg_mode(&base, &base, &head))?;
        run_git(&root, &["checkout", "-q", "-b", "side", &base])?;
        run_git(&root, &["commit", "-q", "--allow-empty", "-m", "diverge"])?;
        let side = git_text(&root, &["rev-parse", "HEAD"])?.trim().to_string();
        run_git(&root, &["checkout", "-q", "--detach", &head])?;
        let not_ancestor = gate_in_ci(&root, "main", &mg_mode(&side, &head, &head))?;
        let _ = fs::remove_dir_all(&root);
        assert_eq!(good.0.violations.len(), 1, "{good:?}");
        assert!(
            good.1.contains("verified GitHub event identity"),
            "{good:?}"
        );
        assert!(!good.1.contains("unavailable"), "{good:?}");
        assert_eq!(wrong_head.0.violations.len(), 1, "{wrong_head:?}");
        assert!(
            wrong_head.1.contains("does not match event head"),
            "{wrong_head:?}"
        );
        assert_eq!(not_ancestor.0.violations.len(), 1, "{not_ancestor:?}");
        assert!(
            not_ancestor.1.contains("not an ancestor"),
            "{not_ancestor:?}"
        );
        Ok(())
    }

    /// merge_group positive: a clean head against an ancestor base passes.
    #[test]
    fn ci_merge_group_passes_a_clean_head() -> Result<(), String> {
        let (root, base, pr, _) =
            pr_event_repo("ci-mg-ok", &[(A, "a seam\n")], &[(A, "a seam\n")])?;
        run_git(&root, &["checkout", "-q", "--detach", &pr])?;
        let (evaluation, note) = gate_in_ci(&root, "main", &mg_mode(&base, &pr, &pr))?;
        let _ = fs::remove_dir_all(&root);
        assert!(evaluation.violations.is_empty(), "{evaluation:?} {note}");
        Ok(())
    }

    fn lookup_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key| {
            owned
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        }
    }

    #[test]
    fn mode_from_lookup_covers_every_environment_branch() -> Result<(), String> {
        let missing_name = Mode::Ci {
            event: Err("GITHUB_EVENT_NAME is missing or invalid under GitHub Actions".into()),
        };
        assert_eq!(Mode::from_lookup(&lookup_of(&[])), Mode::Local);
        assert_eq!(
            Mode::from_lookup(&lookup_of(&[("GITHUB_ACTIONS", "")])),
            Mode::Local
        );
        assert_eq!(
            Mode::from_lookup(&lookup_of(&[("GITHUB_ACTIONS", "true")])),
            missing_name
        );
        assert_eq!(
            Mode::from_lookup(&lookup_of(&[
                ("GITHUB_ACTIONS", "true"),
                ("GITHUB_EVENT_NAME", "")
            ])),
            missing_name
        );
        assert_eq!(
            Mode::from_lookup(&lookup_of(&[("CI", "true"), ("GITHUB_EVENT_NAME", "push")])),
            ci_other()
        );
        assert_eq!(
            Mode::from_lookup(&lookup_of(&[("CI", "true")])),
            Mode::Ci {
                event: Ok(CiEvent {
                    name: String::new(),
                    payload: String::new(),
                    checkout: String::new(),
                })
            }
        );
        assert_eq!(
            Mode::from_lookup(&lookup_of(&[
                ("GITHUB_ACTIONS", "true"),
                ("GITHUB_EVENT_NAME", "pull_request"),
            ])),
            Mode::Ci {
                event: Err("GITHUB_EVENT_PATH is missing".into())
            }
        );
        let dir = std::env::temp_dir().join(format!("ripr-mode-lookup-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|err| format!("mkdir: {err}"))?;
        let path = dir.join("event.json");
        write_file(&path, "{\"k\":1}")?;
        let path_text = path.to_string_lossy().to_string();
        let sha = "a".repeat(40);
        let ok = Mode::from_lookup(&lookup_of(&[
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_EVENT_NAME", "merge_group"),
            ("GITHUB_EVENT_PATH", &path_text),
            ("GITHUB_SHA", &sha),
        ]));
        let no_sha = Mode::from_lookup(&lookup_of(&[
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_EVENT_PATH", &path_text),
        ]));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            ok,
            Mode::Ci {
                event: Ok(CiEvent {
                    name: "merge_group".into(),
                    payload: "{\"k\":1}".into(),
                    checkout: sha,
                })
            }
        );
        assert_eq!(
            no_sha,
            Mode::Ci {
                event: Err("GITHUB_SHA is missing".into())
            }
        );
        Ok(())
    }

    /// An existing clean golden that gains its first banned line is not labelled
    /// a new path.
    #[test]
    fn existing_clean_file_is_not_labelled_a_new_path() {
        let evaluation = delta(&[(A, "plain\n")], &[(A, "a seam\n")], &Baseline::new());
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
        assert!(
            !evaluation.violations[0].contains("new path"),
            "{evaluation:?}"
        );
    }

    /// No upstream ref: loud fallback to absolute ceilings, never a silent pass.
    #[test]
    fn gate_goldens_without_an_upstream_falls_back_loudly_to_absolute_ceilings()
    -> Result<(), String> {
        let root = golden_repo("gate-fallback", &[(A, "a seam\n")], &[(A, "a seam\n")])?;
        let (evaluation, note) = gate_in(&root, "origin/main")?;
        let _ = fs::remove_dir_all(&root);
        assert!(note.contains(BASE_UNAVAILABLE_LABEL), "{note}");
        assert!(
            note.contains("git fetch --deepen=200 origin main"),
            "{note}"
        );
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
        Ok(())
    }

    /// Under CI an unreadable base fails closed even when the absolute ceiling
    /// would pass (growth under a loose ceiling must not read as qualified).
    #[test]
    fn gate_goldens_fails_closed_under_ci_when_the_base_is_unreadable() -> Result<(), String> {
        let root = golden_repo("gate-ci", &[(A, "plain\n")], &[(A, "plain\n")])?;
        let (evaluation, note) = gate_in_ci(&root, "origin/main", &ci_other())?;
        let _ = fs::remove_dir_all(&root);
        assert_eq!(evaluation.violations.len(), 1, "{evaluation:?}");
        assert!(
            note.contains(BASE_UNAVAILABLE_LABEL) && note.contains("closed"),
            "{note}"
        );
        Ok(())
    }

    /// The report records the merge-base, upstream and HEAD shas, and says when
    /// the head side is the working tree.
    #[test]
    fn gate_goldens_records_inputs_and_flags_a_dirty_working_tree() -> Result<(), String> {
        let root = golden_repo("gate-inputs", &[(A, "plain\n")], &[(A, "plain\n")])?;
        let (_, clean) = gate_in(&root, "main")?;
        let head = git_text(&root, &["rev-parse", "HEAD"])?;
        let base = git_text(&root, &["rev-parse", "main"])?;
        assert!(clean.contains(&head.trim()[..12]), "{clean}");
        assert!(clean.contains(&base.trim()[..12]), "{clean}");
        assert!(clean.contains("committed tree"), "{clean}");
        write_file(&root.join(A), "plain\nnew seam\n")?;
        let (dirty_eval, dirty) = gate_in(&root, "main")?;
        let _ = fs::remove_dir_all(&root);
        assert!(dirty.contains("WORKING TREE"), "{dirty}");
        assert_eq!(dirty_eval.violations.len(), 1, "{dirty_eval:?}");
        Ok(())
    }

    /// Quoted and non-ASCII golden paths survive the real `ls-tree -z` read, so a
    /// jargon line in such a file is compared with its base, not treated as new.
    #[test]
    fn gate_goldens_reads_non_ascii_paths_from_the_base() -> Result<(), String> {
        let odd = "fixtures/caf\u{e9} d/expected/human.txt";
        let root = golden_repo("gate-odd", &[(odd, "a seam\n")], &[(odd, "a seam\n")])?;
        let (evaluation, note) = gate_in(&root, "main")?;
        let _ = fs::remove_dir_all(&root);
        assert!(!note.contains("unavailable"), "{note}");
        assert!(evaluation.violations.is_empty(), "{evaluation:?}");
        Ok(())
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

    /// The digest's Evidence line and legend introduce the stage name with its
    /// own gloss (#10261); exempt exactly that, nothing around it.
    #[test]
    fn the_digest_stage_vocabulary_is_exempt_and_nothing_around_it() {
        for exempt in [
            "  Evidence: reach yes · infection yes · propagation yes · observation yes · discriminator weak",
            "Evidence: reach no · infection unknown · propagation yes · observation no · discriminator no",
            "      discriminator = assertion distinguishes intended from wrong behavior",
            "      missing = named discriminator absent from tests; not established = discriminator unconfirmed; not applicable = stage unused",
        ] {
            assert!(
                banned_terms_in_line(exempt).is_empty(),
                "should be exempt: {exempt}"
            );
        }
        for flagged in [
            "Missing check: No strong discriminator was detected",
            "Next step: add a discriminator for this input",
            "Note: observation yes · discriminator weak",
            "Evidence: reach yes · seam weak · discriminator weak",
            "discriminator = something else entirely",
        ] {
            assert!(
                !banned_terms_in_line(flagged).is_empty(),
                "should be flagged: {flagged}"
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

    /// One golden is one subject: the ceiling bounds the worst single file, so
    /// adding goldens never raises a shared total, and a golden that grows past
    /// the per-file ceiling fails on its own.
    #[test]
    fn golden_ceiling_bounds_the_worst_single_file_not_the_sum() -> Result<(), String> {
        let root = temp_root("goldens-per-file")?;
        for name in ["a", "b", "c"] {
            let expected = root.join(format!("fixtures/{name}/expected"));
            write_file(
                &expected.join("human.txt"),
                "a discriminator\nanother discriminator\n",
            )?;
        }
        let mut counts = Counts::new();
        scan_goldens(&root, &mut counts)?;
        let key = (GOLDEN_SURFACE.to_string(), "discriminator".to_string());
        assert_eq!(counts.get(&key).map(|c| c.lines), Some(2));
        let two: Baseline = [(key.clone(), 2)].into_iter().collect();
        assert!(evaluate(&counts, &two).violations.is_empty());
        write_file(
            &root.join("fixtures/d/expected/human.txt"),
            "discriminator\ndiscriminator\ndiscriminator\n",
        )?;
        let mut grown = Counts::new();
        scan_goldens(&root, &mut grown)?;
        let _ = fs::remove_dir_all(&root);
        assert_eq!(grown.get(&key).map(|c| c.lines), Some(3));
        assert!(!evaluate(&grown, &two).violations.is_empty());
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

    /// The committed baseline parses, every row names a known surface and term,
    /// the committed goldens scan, and the header still matches what `--ratchet`
    /// would write. Staying under the absolute ceiling is deliberately not
    /// asserted (#11374): the gate is each PR's delta.
    #[test]
    fn committed_baseline_rows_are_known_and_the_header_matches_the_renderer() -> Result<(), String>
    {
        let root = repo_root()?;
        let text = fs::read_to_string(root.join(BASELINE_PATH))
            .map_err(|err| format!("read baseline: {err}"))?;
        let baseline = parse_baseline(&text)?;
        assert!(!baseline.is_empty(), "baseline has no rows");
        for (surface, term) in baseline.keys() {
            assert!(
                surface == GOLDEN_SURFACE
                    || surface
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
                "unknown surface `{surface}`"
            );
            assert!(BANNED.iter().any(|banned| banned.id == term), "term {term}");
        }
        assert_eq!(render_baseline(&baseline), text, "header or format drifted");
        let mut counts = Counts::new();
        let scanned = scan_goldens(&root, &mut counts)?;
        assert!(scanned > 0, "no default human.txt goldens were scanned");
        assert!(
            !counts.is_empty(),
            "committed goldens scanned to zero banned lines"
        );
        Ok(())
    }
}

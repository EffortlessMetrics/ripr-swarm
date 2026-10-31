//! `cargo xtask fold-changelog [--dry-run | --write] [--limit N]
//! [--keep-fragments] [--release-cut]` (#5114): the repo-owned fold step of the release cut.
//! It appends each `changelog.d/*.md` entry (minus its section line) to the
//! first `### <Section>` heading under `## Unreleased` in `CHANGELOG.md`, in
//! file-name order, then deletes the folded fragment. The default writes
//! nothing. Fragment validation is the `check-changelog-fragments` owner's
//! `fragment_violations`, so the fold cannot accept what the check refuses.

use std::fs;
use std::path::Path;

use super::changelog_fragments::{
    ALLOWED_SECTIONS, CHANGELOG, FRAGMENT_DIR, README, changelog_fragment_violations,
    fragment_violations, section_of,
};

const USAGE: &str = "usage: cargo xtask fold-changelog [--dry-run | --write] [--limit <N>] [--keep-fragments] [--release-cut]";
/// Candidate-admission ceiling on changed files in one PR.
const ADMISSION_FILE_CAP: usize = 160;
const UNRELEASED: &str = "## Unreleased";

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FoldOptions {
    pub(super) write: bool,
    pub(super) limit: Option<usize>,
    pub(super) keep_fragments: bool,
    /// Cut-day mode: fold every fragment, then require the release-cut check
    /// to pass, so one command leaves `changelog.d/` empty and verified.
    pub(super) release_cut: bool,
}

/// `(file name, section, entry count)`.
pub(super) type FoldedFragment = (String, String, usize);

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FoldReport {
    /// `(file name, section, entry count)` in fold order.
    pub(super) folded: Vec<FoldedFragment>,
    /// Fragments left for a later batch by `--limit`.
    pub(super) remaining: usize,
    pub(super) changelog: String,
}

pub(crate) fn fold_changelog(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let report = fold_in(Path::new("."), &options)?;
    let verb = if options.write {
        "folded"
    } else {
        "would fold"
    };
    for (name, section, entries) in &report.folded {
        println!("{verb} {FRAGMENT_DIR}/{name} -> `### {section}` ({entries} entries)");
    }
    println!(
        "{verb} {} fragment(s); {} left in {FRAGMENT_DIR}/",
        report.folded.len(),
        report.remaining
    );
    if !options.write {
        println!("dry run: nothing written; pass --write to apply");
    } else if options.keep_fragments {
        println!("--keep-fragments: fragment files were not deleted");
    }
    if options.release_cut {
        // The PR carries every deleted fragment plus CHANGELOG.md.
        if report.folded.len() + 1 > ADMISSION_FILE_CAP {
            println!(
                "warning: {} changed files exceeds the {ADMISSION_FILE_CAP}-file admission cap; \
                 fold a snapshot ahead of the cut with `--write --keep-fragments --limit` and \
                 delete in batches (changelog.d/README.md)",
                report.folded.len() + 1
            );
        }
        if options.write {
            let violations = changelog_fragment_violations(Path::new("."), true)?;
            if !violations.is_empty() {
                return Err(format!(
                    "release-cut check failed after the fold:\n  {}",
                    violations.join("\n  ")
                ));
            }
            println!("release-cut check passed: {FRAGMENT_DIR}/ holds no fragments");
        } else {
            println!("dry run: the release-cut check runs after --write");
        }
    }
    Ok(())
}

pub(super) fn parse_args(args: &[String]) -> Result<FoldOptions, String> {
    let mut write = false;
    let mut dry_run = false;
    let mut keep_fragments = false;
    let mut release_cut = false;
    let mut limit = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--write" => write = true,
            "--dry-run" => dry_run = true,
            "--keep-fragments" => keep_fragments = true,
            "--release-cut" => release_cut = true,
            "--limit" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("--limit needs a number\n{USAGE}"))?;
                limit = Some(parse_limit(value)?);
            }
            other => match other.strip_prefix("--limit=") {
                Some(value) => limit = Some(parse_limit(value)?),
                None => return Err(format!("unknown argument `{other}`\n{USAGE}")),
            },
        }
    }
    if write && dry_run {
        return Err(format!("--write and --dry-run conflict\n{USAGE}"));
    }
    if release_cut && (limit.is_some() || keep_fragments) {
        return Err(format!(
            "--release-cut folds and deletes every fragment, so it conflicts with --limit and --keep-fragments\n{USAGE}"
        ));
    }
    Ok(FoldOptions {
        write,
        limit,
        keep_fragments,
        release_cut,
    })
}

fn parse_limit(value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!(
            "--limit needs a positive whole number, found `{value}`\n{USAGE}"
        )),
    }
}

pub(super) fn fold_in(root: &Path, options: &FoldOptions) -> Result<FoldReport, String> {
    let changelog_path = root.join(CHANGELOG);
    let changelog = fs::read_to_string(&changelog_path)
        .map_err(|err| format!("failed to read {CHANGELOG}: {err}"))?;
    let dir = root.join(FRAGMENT_DIR);
    let entries =
        fs::read_dir(&dir).map_err(|err| format!("failed to read {FRAGMENT_DIR}/: {err}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read {FRAGMENT_DIR}/: {err}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == README {
            continue;
        }
        let is_file = entry
            .file_type()
            .map_err(|err| format!("failed to stat {FRAGMENT_DIR}/{name}: {err}"))?
            .is_file();
        if !is_file {
            return Err(format!(
                "{FRAGMENT_DIR}/{name}: only fragment files belong here, not directories or links"
            ));
        }
        names.push(name);
    }
    names.sort();
    let total = names.len();
    if let Some(limit) = options.limit {
        names.truncate(limit);
    }
    let mut fragments = Vec::new();
    for name in &names {
        let text = fs::read_to_string(dir.join(name))
            .map_err(|err| format!("failed to read {FRAGMENT_DIR}/{name}: {err}"))?;
        fragments.push((name.clone(), text));
    }
    let (folded_text, folded) = fold_text(&changelog, &fragments)?;
    if options.write && !folded.is_empty() {
        fs::write(&changelog_path, &folded_text)
            .map_err(|err| format!("failed to write {CHANGELOG}: {err}"))?;
        if !options.keep_fragments {
            for name in &names {
                fs::remove_file(dir.join(name))
                    .map_err(|err| format!("failed to delete {FRAGMENT_DIR}/{name}: {err}"))?;
            }
        }
    }
    Ok(FoldReport {
        remaining: total - folded.len(),
        folded,
        changelog: folded_text,
    })
}

/// One validated fragment: its section and entry lines (no blank lines).
fn parse_fragment(name: &str, text: &str) -> Result<(String, Vec<String>), String> {
    let violations = fragment_violations(name, text);
    if !violations.is_empty() {
        return Err(format!(
            "refusing to fold {FRAGMENT_DIR}/{name}:\n  {}",
            violations.join("\n  ")
        ));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let section = lines.next().and_then(section_of).unwrap_or_default();
    if !ALLOWED_SECTIONS.contains(&section) {
        return Err(format!(
            "refusing to fold {FRAGMENT_DIR}/{name}: unknown section `{section}`"
        ));
    }
    let body = lines
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    Ok((section.to_string(), body))
}

pub(super) fn fold_text(
    changelog: &str,
    fragments: &[(String, String)],
) -> Result<(String, Vec<FoldedFragment>), String> {
    let mut lines: Vec<String> = changelog.lines().map(str::to_string).collect();
    let trailing_newline = changelog.ends_with('\n');
    let mut folded = Vec::new();
    for (name, text) in fragments {
        let (section, body) = parse_fragment(name, text)?;
        let (start, end) = unreleased_range(&lines)?;
        let block = lines[start..end].join("\n");
        let entry_text = body.join("\n");
        if block.contains(&entry_text) {
            return Err(format!(
                "refusing to fold {FRAGMENT_DIR}/{name}: its entry text is already in `{UNRELEASED}` (folded earlier?); delete the fragment or fix the duplicate"
            ));
        }
        let heading = format!("### {section}");
        let at = match lines[start..end].iter().position(|line| *line == heading) {
            Some(offset) => {
                let from = start + offset + 1;
                let next = lines[from..end]
                    .iter()
                    .position(|line| line.starts_with("## ") || line.starts_with("### "))
                    .map_or(end, |n| from + n);
                last_content_line(&lines, from, next)
            }
            None => {
                let after = last_content_line(&lines, start + 1, end);
                lines.splice(after..after, [String::new(), heading.clone()]);
                after + 2
            }
        };
        let mut insert = body.clone();
        // A heading with no entries yet needs a blank line before its first.
        if lines[at - 1] == heading || lines[at - 1].is_empty() {
            insert.insert(0, String::new());
        }
        let count = body.iter().filter(|line| line.starts_with("- ")).count();
        let from = at;
        lines.splice(from..from, insert);
        folded.push((name.clone(), section, count));
    }
    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    Ok((out, folded))
}

/// Line range `[after "## Unreleased", next "## " or end)`; the start is the
/// heading line itself.
fn unreleased_range(lines: &[String]) -> Result<(usize, usize), String> {
    let start = lines
        .iter()
        .position(|line| line.trim() == UNRELEASED)
        .ok_or_else(|| format!("{CHANGELOG} has no `{UNRELEASED}` heading"))?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.starts_with("## "))
        .map_or(lines.len(), |n| start + 1 + n);
    Ok((start, end))
}

/// Index just past the last non-blank line in `[from, to)`, or `from` when
/// the range is blank.
fn last_content_line(lines: &[String], from: usize, to: usize) -> usize {
    (from..to)
        .rev()
        .find(|&i| !lines[i].trim().is_empty())
        .map_or(from, |i| i + 1)
}

#[cfg(test)]
mod tests;

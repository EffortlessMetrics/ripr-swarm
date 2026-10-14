//! `cargo xtask check-changelog-fragments` (#6341, #7671, #7674): every
//! `changelog.d/` fragment must fold into `CHANGELOG.md` without a reader
//! having to guess, every `CHANGELOG.md` Unreleased bullet must already be
//! traceable, and verdict-corpus / `Repo ops:` bookkeeping must not crowd
//! user-facing sections. A fragment with no section line, an unknown section,
//! or no issue/PR reference used to pass every gate and then drop out of the
//! release notes at the fold. Unreleased entries were exempt from that
//! reference rule, so folded history could ship untraceable. `--release-cut`
//! additionally fails on any fragment at all, so the fold step is a command
//! that fails rather than a checklist line.

use std::fs;
use std::path::Path;

use crate::{FixKind, PolicyReportSpec, finish_policy_report};

const FRAGMENT_DIR: &str = "changelog.d";
const README: &str = "README.md";
const POLICY_DOC: &str = "docs/CHANGELOG_POLICY.md";
const CHANGELOG: &str = "CHANGELOG.md";

/// The section headings `docs/CHANGELOG_POLICY.md` allows. The check also
/// fails when the policy doc stops listing one of them, so the two cannot
/// drift apart silently.
const ALLOWED_SECTIONS: &[&str] = &[
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
    "Docs",
    "Internal / CI",
];

const INTERNAL_CI_SECTION: &str = "Internal / CI";

const USAGE: &str = "usage: cargo xtask check-changelog-fragments [--release-cut]";

pub(crate) fn check_changelog_fragments() -> Result<(), String> {
    check_changelog_fragments_with_args(&[])
}

pub(crate) fn check_changelog_fragments_with_args(args: &[String]) -> Result<(), String> {
    let release_cut = match args {
        [] => false,
        [flag] if flag == "--release-cut" => true,
        _ => return Err(USAGE.to_string()),
    };
    let violations = changelog_fragment_violations(Path::new("."), release_cut)?;
    finish_policy_report(
        PolicyReportSpec {
            report_file: "changelog-fragments.md",
            check: "check-changelog-fragments",
            why_it_matters: "Fragments are folded into CHANGELOG.md by hand at the release cut. A fragment with no section, an unknown section, or no issue/PR reference cannot be placed or traced, a fragment left unfolded drops a user-facing change from the release notes, an Unreleased bullet with no issue, PR, or RIPR-SPEC reference is untraceable after the cut, and verdict-corpus or Repo ops bookkeeping in a user-facing section crowds the notes an upgrader reads.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Start the fragment with `<!-- section: X -->`, where X is one of the sections in docs/CHANGELOG_POLICY.md.",
                "Write the entry as a `- ` bullet with the issue or PR reference (`#N` or a link) in it.",
                "Name the file `<pr-or-issue>-<short-slug>.md` in lowercase kebab case.",
                "File `Verdict corpus:` and `Repo ops:` bookkeeping under `Internal / CI`, not Added, Changed, or Fixed.",
                "Give each CHANGELOG.md Unreleased bullet an issue, PR (`#N` or a link), or RIPR-SPEC-N reference.",
                "At the release cut, fold every fragment into CHANGELOG.md and delete it (changelog.d/README.md).",
            ],
            rerun_command: if release_cut {
                "cargo xtask check-changelog-fragments --release-cut"
            } else {
                "cargo xtask check-changelog-fragments"
            },
            exception_template: None,
        },
        &violations,
    )
}

pub(crate) fn changelog_fragment_violations(
    root: &Path,
    release_cut: bool,
) -> Result<Vec<String>, String> {
    let mut violations = Vec::new();

    let policy_path = root.join(POLICY_DOC);
    let policy = fs::read_to_string(&policy_path)
        .map_err(|err| format!("failed to read {POLICY_DOC}: {err}"))?;
    let documented = documented_sections(&policy);
    for section in ALLOWED_SECTIONS {
        if !documented.contains(section) {
            violations.push(format!(
                "{POLICY_DOC} no longer lists section `{section}`; update ALLOWED_SECTIONS in xtask/src/policy/changelog_fragments.rs to match"
            ));
        }
    }
    for section in documented
        .iter()
        .filter(|section| !ALLOWED_SECTIONS.contains(section))
    {
        violations.push(format!(
            "{POLICY_DOC} lists section `{section}` that fragments cannot use; add it to ALLOWED_SECTIONS in xtask/src/policy/changelog_fragments.rs"
        ));
    }

    let dir = root.join(FRAGMENT_DIR);
    let entries =
        fs::read_dir(&dir).map_err(|err| format!("failed to read {FRAGMENT_DIR}/: {err}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read {FRAGMENT_DIR}/: {err}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to stat {FRAGMENT_DIR}/{name}: {err}"))?;
        if !file_type.is_file() {
            violations.push(format!(
                "{FRAGMENT_DIR}/{name}: only fragment files belong here, not directories or links"
            ));
            continue;
        }
        names.push(name);
    }
    names.sort();

    if !names.iter().any(|name| name == README) {
        violations.push(format!(
            "{FRAGMENT_DIR}/{README} is missing; it documents the fragment format and the fold"
        ));
    }
    for name in names.iter().filter(|name| *name != README) {
        let path = format!("{FRAGMENT_DIR}/{name}");
        let text = fs::read_to_string(dir.join(name))
            .map_err(|err| format!("failed to read {path}: {err}"))?;
        violations.extend(fragment_violations(name, &text));
        if release_cut {
            violations.push(format!(
                "{path}: unfolded at the release cut; fold it into CHANGELOG.md and delete it (changelog.d/README.md)"
            ));
        }
    }
    violations.extend(unreleased_changelog_violations(root)?);
    Ok(violations)
}

/// The `changelog.d/` paths in a tree listing that are still unfolded:
/// everything but `README.md`. Release tooling (source promotion and
/// back-sync, #6341) reads Git trees rather than the working tree, so it
/// lists the paths itself and asks this owner which of them block a release.
pub(crate) fn unfolded_fragment_paths<'a>(
    tree_paths: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let readme = format!("{FRAGMENT_DIR}/{README}");
    let prefix = format!("{FRAGMENT_DIR}/");
    let mut paths: Vec<String> = tree_paths
        .into_iter()
        .filter(|path| path.starts_with(&prefix) && *path != readme)
        .map(str::to_string)
        .collect();
    paths.sort();
    paths
}

/// Violations for one fragment file, given its file name and contents.
fn fragment_violations(name: &str, text: &str) -> Vec<String> {
    let path = format!("{FRAGMENT_DIR}/{name}");
    let mut violations = Vec::new();

    match name.strip_suffix(".md") {
        None => violations.push(format!("{path}: fragments must be `.md` files")),
        Some(stem) => {
            if !is_kebab_slug(stem) {
                violations.push(format!(
                    "{path}: name must be lowercase kebab case, for example `5188-first-pr-quoting.md`"
                ));
            } else if stem
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'-')
            {
                violations.push(format!(
                    "{path}: name needs a slug, not just a number; two PRs for one issue would collide"
                ));
            }
        }
    }

    // An editor-added byte-order mark would otherwise make a valid section
    // line fail with a message that prints the same visible text.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let section = match section_of(first) {
        Some(section) if ALLOWED_SECTIONS.contains(&section) => Some(section),
        Some(section) => {
            violations.push(format!(
                "{path}: unknown section `{section}`; use one of {}",
                ALLOWED_SECTIONS.join(", ")
            ));
            None
        }
        None => {
            violations.push(format!(
                "{path}: first line must be `<!-- section: X -->`, found `{first}`"
            ));
            None
        }
    };

    let body: Vec<&str> = lines.collect();
    // HTML comments are copied into CHANGELOG.md verbatim at the fold, so a
    // second section line or a reference hidden in a comment is not an entry.
    if body.iter().any(|line| line.contains("<!--")) {
        violations.push(format!(
            "{path}: only the first line may hold an HTML comment; one fragment holds one section"
        ));
    }
    // Each `- ` line at column 0 opens an entry; indented lines continue it.
    // Every entry is folded into the release notes on its own, so each one
    // must be a bullet and carry its own issue or PR reference.
    let mut entries: Vec<Vec<&str>> = Vec::new();
    for line in &body {
        // Blank lines separate entries; comment lines are reported above.
        if line.trim().is_empty() || line.trim_start().starts_with("<!--") {
            continue;
        }
        let continues = line.starts_with(char::is_whitespace) && !entries.is_empty();
        if continues {
            if let Some(entry) = entries.last_mut() {
                entry.push(line);
            }
            continue;
        }
        if !line.starts_with("- ") {
            violations.push(format!(
                "{path}: entry must start with a `- ` bullet at column 0, found `{line}`"
            ));
        } else if line[2..].trim().is_empty() {
            violations.push(format!("{path}: the bullet line has no entry text"));
        }
        entries.push(vec![line]);
    }
    if entries.is_empty() {
        violations.push(format!("{path}: entry is empty"));
    }
    for entry in &entries {
        if !entry
            .iter()
            .filter(|line| !line.contains("<!--"))
            .any(|line| has_issue_reference(line))
        {
            violations.push(format!(
                "{path}: entry `{}` names no issue or PR; add `(#N)` or a link to it",
                entry[0].trim()
            ));
        }
        if is_internal_bookkeeping_line(entry[0])
            && section.is_some_and(|section| section != INTERNAL_CI_SECTION)
        {
            violations.push(format!(
                "{path}: entry `{}` is corpus or Repo ops bookkeeping; use `<!-- section: {INTERNAL_CI_SECTION} -->` instead of a user-facing section",
                entry[0].trim()
            ));
        }
    }
    violations
}

/// Unreleased `CHANGELOG.md` bullets that name no issue, PR, or RIPR-SPEC
/// (#7671), or that file corpus/`Repo ops:` bookkeeping under a user-facing
/// heading (#7674). Missing `CHANGELOG.md` is skipped so fragment-only
/// fixtures stay valid; versioned sections after Unreleased are not scanned.
fn unreleased_changelog_violations(root: &Path) -> Result<Vec<String>, String> {
    let path = root.join(CHANGELOG);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&path).map_err(|err| format!("failed to read {CHANGELOG}: {err}"))?;
    Ok(unreleased_entry_violations(&text))
}

fn unreleased_entry_violations(text: &str) -> Vec<String> {
    let mut in_unreleased = false;
    let mut heading = String::new();
    let mut entries: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if line.trim() == "## Unreleased" {
            in_unreleased = true;
            continue;
        }
        if in_unreleased && line.starts_with("## ") {
            break;
        }
        if !in_unreleased {
            continue;
        }
        if let Some(name) = line.strip_prefix("### ") {
            heading = name.trim().to_string();
            continue;
        }
        if line.trim().is_empty() || line.trim_start().starts_with("<!--") {
            continue;
        }
        let continues = line.starts_with(char::is_whitespace) && !entries.is_empty();
        if continues {
            if let Some((_, entry)) = entries.last_mut() {
                entry.push(line);
            }
            continue;
        }
        if line.starts_with("- ") {
            entries.push((heading.clone(), vec![line]));
        }
    }

    let mut violations = Vec::new();
    for (heading, entry) in &entries {
        if !entry
            .iter()
            .filter(|line| !line.contains("<!--"))
            .any(|line| has_issue_reference(line) || has_spec_reference(line))
        {
            violations.push(format!(
                "{CHANGELOG}: Unreleased entry `{}` names no issue, PR, or RIPR-SPEC; add `(#N)` or a RIPR-SPEC id",
                entry[0].trim()
            ));
        }
        if is_internal_bookkeeping_line(entry[0]) && heading.as_str() != INTERNAL_CI_SECTION {
            violations.push(format!(
                "{CHANGELOG}: Unreleased entry `{}` is corpus or Repo ops bookkeeping; move it under `### {INTERNAL_CI_SECTION}` instead of a user-facing heading",
                entry[0].trim()
            ));
        }
    }
    violations
}

/// First-line prefixes that belong under `Internal / CI` (#7674). Analyzer
/// behavior a corpus case happens to prove, and `ripr init --ci` workflow
/// notes, stay user-facing.
fn is_internal_bookkeeping_line(line: &str) -> bool {
    let text = line.strip_prefix("- ").unwrap_or(line).trim_start();
    text.starts_with("Verdict corpus:")
        || text.starts_with("Repo ops:")
        || text.starts_with("Labeled Rust verdict corpus")
}

/// The backticked items of the `## Sections` list in the policy doc.
fn documented_sections(policy: &str) -> Vec<&str> {
    policy
        .lines()
        .skip_while(|line| line.trim() != "## Sections")
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.trim().strip_prefix("- `")?.strip_suffix('`'))
        .collect()
}

fn section_of(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim()
        .strip_prefix("section:")
        .map(str::trim)
}

fn is_kebab_slug(stem: &str) -> bool {
    !stem.is_empty()
        && stem.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// `#123` as its own token (after the line start, a space, `(` or `[`, so a
/// heading anchor such as `X.md#3-step` or a color such as `#1f2937` does not
/// count), or a GitHub `/issues/123` or `/pull/123` link.
fn has_issue_reference(line: &str) -> bool {
    let followed_by_digit = |rest: &str| {
        rest.bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
    };
    let bare_issue_number = |rest: &str| {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0
            && rest[digits..]
                .bytes()
                .next()
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && byte != b'-')
    };
    line.match_indices('#').any(|(index, _)| {
        let before = line[..index].bytes().next_back();
        before.is_none_or(|byte| byte.is_ascii_whitespace() || byte == b'(' || byte == b'[')
            && bare_issue_number(&line[index + 1..])
    }) || ["/issues/", "/pull/"].iter().any(|marker| {
        line.match_indices(marker)
            .any(|(index, _)| followed_by_digit(&line[index + marker.len()..]))
    })
}

/// `RIPR-SPEC-12` is a traceable Unreleased reference (#7671). A trailing
/// digit is required so a mention of the prefix alone does not count.
fn has_spec_reference(line: &str) -> bool {
    const MARKER: &str = "RIPR-SPEC-";
    line.match_indices(MARKER).any(|(index, _)| {
        line[index + MARKER.len()..]
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_ROOTS: AtomicUsize = AtomicUsize::new(0);

    /// A fresh directory per call, so a leftover from a crashed run cannot add
    /// stray fragments to an exact-count assertion.
    fn fresh_root(label: &str) -> Result<std::path::PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-{label}-{}-{}",
            std::process::id(),
            TEMP_ROOTS.fetch_add(1, Ordering::Relaxed)
        ));
        if root.exists() {
            fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
        }
        Ok(root)
    }

    const GOOD: &str = "<!-- section: Fixed -->\n- `ripr first-pr` quotes the root (#5188).\n";

    #[test]
    fn well_formed_fragment_passes() {
        assert_eq!(
            fragment_violations("5188-first-pr-quoting.md", GOOD),
            Vec::<String>::new()
        );
        let linked = "<!-- section: Docs -->\n- A change\n  ([#5471](https://github.com/o/r/issues/5471)).\r\n";
        assert_eq!(
            fragment_violations("agent-stub.md", linked),
            Vec::<String>::new()
        );
        let pull = "<!-- section: Added -->\n- See https://github.com/o/r/pull/12.\n";
        assert_eq!(
            fragment_violations("pull-link.md", pull),
            Vec::<String>::new()
        );
    }

    #[test]
    fn issue_repro_fragment_fails_on_section_and_reference() {
        // The #6341 repro: `hello` passed every gate before this check.
        let violations = fragment_violations("x.md", "hello\n");
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations[0].contains("first line must be `<!-- section: X -->`"));
        assert!(violations[1].contains("entry is empty"));
    }

    #[test]
    fn unknown_section_is_named() {
        let text = "<!-- section: Improvements -->\n- Faster (#1).\n";
        let violations = fragment_violations("faster.md", text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("unknown section `Improvements`"));
    }

    #[test]
    fn internal_ci_section_is_allowed() {
        let text = "<!-- section: Internal / CI -->\n- Verdict corpus: 14 cases (#1).\n";
        assert_eq!(
            fragment_violations("corpus-batch.md", text),
            Vec::<String>::new()
        );
        let repo_ops = "<!-- section: Internal / CI -->\n- Repo ops: dx-scoreboard (#1).\n";
        assert_eq!(
            fragment_violations("dx-scoreboard.md", repo_ops),
            Vec::<String>::new()
        );
    }

    #[test]
    fn verdict_corpus_bookkeeping_belongs_in_internal_ci() {
        let added = "<!-- section: Added -->\n- Verdict corpus: 14 cases (#1).\n";
        let violations = fragment_violations("corpus-batch.md", added);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("Internal / CI"), "{violations:?}");

        let repo_ops = "<!-- section: Fixed -->\n- Repo ops: dx-scoreboard (#1).\n";
        let violations = fragment_violations("dx-scoreboard.md", repo_ops);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("Internal / CI"), "{violations:?}");

        // Analyzer behavior a corpus case happens to prove stays user-facing.
        let behavior = "<!-- section: Fixed -->\n- The Rust verdict corpus describes the checkout daily-limit boundary oracle (#6639).\n";
        assert_eq!(
            fragment_violations("module-path-oracle.md", behavior),
            Vec::<String>::new()
        );
    }

    #[test]
    fn entry_must_be_a_bullet_with_a_reference() {
        let prose = "<!-- section: Fixed -->\nFixed a thing (#1).\n";
        assert!(
            fragment_violations("prose.md", prose)[0].contains("must start with a `- ` bullet")
        );
        // A bare `#` or an issue link without a number is not a reference.
        let unreferenced = "<!-- section: Fixed -->\n- Fixed # things, see /issues/ list.\n";
        let violations = fragment_violations("no-ref.md", unreferenced);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("names no issue or PR"));
    }

    #[test]
    fn anchors_colors_and_comments_are_not_references() {
        for body in [
            "- See docs/X.md#3-step for the steps.",
            "- Badge color is now #1f2937.",
            "- Fixed `foo#1` parsing.",
            "- Fix color `#123456`.",
        ] {
            let text = format!("<!-- section: Fixed -->\n{body}\n");
            let violations = fragment_violations("not-a-ref.md", &text);
            assert_eq!(violations.len(), 1, "{body}: {violations:?}");
            assert!(violations[0].contains("names no issue or PR"), "{body}");
        }
        let hidden = "<!-- section: Fixed -->\n- x.\n<!-- #12 -->\n";
        let violations = fragment_violations("hidden.md", hidden);
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations[0].contains("only the first line may hold an HTML comment"));
        let inline = "<!-- section: Fixed -->\n- Fixed something <!-- (#12) -->\n";
        let violations = fragment_violations("inline.md", inline);
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations[1].contains("names no issue or PR"));
        let two_sections =
            "<!-- section: Fixed -->\n- x (#1).\n<!-- section: Added -->\n- y (#2).\n";
        let violations = fragment_violations("two.md", two_sections);
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn every_entry_is_a_bullet_with_its_own_reference() {
        let prose = fragment_violations(
            "1-two.md",
            "<!-- section: Fixed -->\n- First fix (#1).\n\nSecond fix (#2).\n",
        );
        assert!(
            prose
                .iter()
                .any(|v| v.contains("must start with a `- ` bullet") && v.contains("Second fix")),
            "{prose:?}"
        );
        let unreferenced = fragment_violations(
            "1-two.md",
            "<!-- section: Fixed -->\n- First fix (#1).\n- Second fix.\n",
        );
        assert_eq!(
            unreferenced,
            vec![
                "changelog.d/1-two.md: entry `- Second fix.` names no issue or PR; add `(#N)` or a link to it"
                    .to_string()
            ]
        );
        let wrapped = fragment_violations(
            "1-two.md",
            "<!-- section: Fixed -->\n- First fix\n  wraps here (#1).\n  - nested detail\n- Second fix (#2).\n",
        );
        assert!(wrapped.is_empty(), "{wrapped:?}");
    }

    #[test]
    fn bullet_needs_entry_text() {
        let text = "<!-- section: Fixed -->\n- \n  (#1).\n";
        let violations = fragment_violations("empty-bullet.md", text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("no entry text"));
    }

    #[test]
    fn byte_order_mark_and_indented_bullet() {
        assert_eq!(
            fragment_violations("bom.md", &format!("\u{feff}{GOOD}")),
            Vec::<String>::new()
        );
        let indented = "<!-- section: Fixed -->\n  - x (#1).\n";
        let violations = fragment_violations("indented.md", indented);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].ends_with("found `  - x (#1).`"),
            "{violations:?}"
        );
    }

    #[test]
    fn names_must_be_kebab_slugs_with_text() {
        for bad in [
            "Fix.md",
            "fix_thing.md",
            "fix--thing.md",
            "-fix.md",
            "fix.txt",
        ] {
            assert_eq!(
                fragment_violations(bad, GOOD).len(),
                1,
                "{bad} should be rejected"
            );
        }
        assert!(fragment_violations("5188.md", GOOD)[0].contains("needs a slug"));
        assert!(fragment_violations("5188-6000.md", GOOD)[0].contains("needs a slug"));
    }

    #[test]
    fn release_cut_rejects_any_remaining_fragment() -> Result<(), String> {
        let root = fresh_root("changelog-fragments")?;
        let dir = root.join(FRAGMENT_DIR);
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        fs::create_dir_all(root.join("docs")).map_err(|err| err.to_string())?;
        let policy: String = std::iter::once("## Sections\n\n".to_string())
            .chain(ALLOWED_SECTIONS.iter().map(|s| format!("- `{s}`\n")))
            .collect();
        fs::write(root.join(POLICY_DOC), policy).map_err(|err| err.to_string())?;
        fs::write(dir.join(README), "# fragments\n").map_err(|err| err.to_string())?;

        let only_readme = changelog_fragment_violations(&root, true);
        fs::write(dir.join("5188-first-pr-quoting.md"), GOOD).map_err(|err| err.to_string())?;
        let ordinary = changelog_fragment_violations(&root, false);
        let cut = changelog_fragment_violations(&root, true);
        let _ = fs::remove_dir_all(&root);

        assert_eq!(only_readme?, Vec::<String>::new());
        assert_eq!(ordinary?, Vec::<String>::new());
        let cut = cut?;
        assert_eq!(cut.len(), 1, "{cut:?}");
        assert!(
            cut[0].starts_with("changelog.d/5188-first-pr-quoting.md: unfolded at the release cut")
        );
        Ok(())
    }

    #[test]
    fn policy_doc_drift_is_reported() -> Result<(), String> {
        let root = fresh_root("changelog-policy-drift")?;
        fs::create_dir_all(root.join(FRAGMENT_DIR)).map_err(|err| err.to_string())?;
        fs::create_dir_all(root.join("docs")).map_err(|err| err.to_string())?;
        fs::write(
            root.join(POLICY_DOC),
            "## Sections\n\n- `Added`\n- `Improved`\n\n## Static Language\n\n- `Docs`\n",
        )
        .map_err(|err| err.to_string())?;
        fs::write(root.join(FRAGMENT_DIR).join(README), "x\n").map_err(|err| err.to_string())?;
        let violations = changelog_fragment_violations(&root, false);
        let _ = fs::remove_dir_all(&root);
        let violations = violations?;
        // Every allowed section is missing from the list (a `Docs` item under
        // a later heading does not count) and one listed section is unknown.
        assert_eq!(violations.len(), ALLOWED_SECTIONS.len(), "{violations:?}");
        assert!(
            violations
                .iter()
                .any(|v| v.contains("lists section `Improved` that fragments cannot use"))
        );
        assert!(
            violations
                .iter()
                .any(|v| v.contains("no longer lists section `Docs`"))
        );
        Ok(())
    }

    #[test]
    fn unreleased_bullet_needs_a_traceable_reference() {
        let missing = "# Changelog\n\n## Unreleased\n\n### Fixed\n\n- Faster checks.\n";
        let violations = unreleased_entry_violations(missing);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].contains("names no issue, PR, or RIPR-SPEC"),
            "{violations:?}"
        );

        let issue = "# Changelog\n\n## Unreleased\n\n### Fixed\n\n- Faster checks (#12).\n";
        assert_eq!(unreleased_entry_violations(issue), Vec::<String>::new());

        let spec = "# Changelog\n\n## Unreleased\n\n### Added\n\n- Corpus cases, RIPR-SPEC-0219.\n";
        assert_eq!(unreleased_entry_violations(spec), Vec::<String>::new());

        let wrapped = "# Changelog\n\n## Unreleased\n\n### Fixed\n\n- Faster checks\n  wrap onto the next line (#12).\n";
        assert_eq!(unreleased_entry_violations(wrapped), Vec::<String>::new());

        let prefix_only =
            "# Changelog\n\n## Unreleased\n\n### Docs\n\n- Mentions RIPR-SPEC- without a number.\n";
        assert_eq!(unreleased_entry_violations(prefix_only).len(), 1);

        let versioned_only = "# Changelog\n\n## Unreleased\n\n### Fixed\n\n- Traced (#1).\n\n## 0.10.0\n\n### Fixed\n\n- Historical and untraced.\n";
        assert_eq!(
            unreleased_entry_violations(versioned_only),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unreleased_user_facing_rejects_verdict_corpus_bookkeeping() {
        let crowded =
            "# Changelog\n\n## Unreleased\n\n### Added\n\n- Verdict corpus: 14 cases (#12).\n";
        let violations = unreleased_entry_violations(crowded);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("Internal / CI"), "{violations:?}");

        let segregated = "# Changelog\n\n## Unreleased\n\n### Internal / CI\n\n- Verdict corpus: 14 cases (#12).\n";
        assert_eq!(
            unreleased_entry_violations(segregated),
            Vec::<String>::new()
        );

        let repo_ops =
            "# Changelog\n\n## Unreleased\n\n### Changed\n\n- Repo ops: dx-scoreboard (#12).\n";
        assert_eq!(unreleased_entry_violations(repo_ops).len(), 1);

        let user_facing_ci = "# Changelog\n\n## Unreleased\n\n### Changed\n\n- CI: the `ripr init --ci github` workflow (#12).\n";
        assert_eq!(
            unreleased_entry_violations(user_facing_ci),
            Vec::<String>::new()
        );
    }

    #[test]
    fn changelog_unreleased_scan_runs_through_the_fragment_check() -> Result<(), String> {
        let root = fresh_root("changelog-unreleased-refs")?;
        fs::create_dir_all(root.join(FRAGMENT_DIR)).map_err(|err| err.to_string())?;
        fs::create_dir_all(root.join("docs")).map_err(|err| err.to_string())?;
        let policy: String = std::iter::once("## Sections\n\n".to_string())
            .chain(ALLOWED_SECTIONS.iter().map(|s| format!("- `{s}`\n")))
            .collect();
        fs::write(root.join(POLICY_DOC), policy).map_err(|err| err.to_string())?;
        fs::write(root.join(FRAGMENT_DIR).join(README), "# fragments\n")
            .map_err(|err| err.to_string())?;
        fs::write(
            root.join(CHANGELOG),
            "# Changelog\n\n## Unreleased\n\n### Fixed\n\n- No reference here.\n",
        )
        .map_err(|err| err.to_string())?;

        let violations = changelog_fragment_violations(&root, false);
        let _ = fs::remove_dir_all(&root);
        let violations = violations?;
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].starts_with("CHANGELOG.md: Unreleased entry"),
            "{violations:?}"
        );
        Ok(())
    }

    #[test]
    fn changelog_internal_bookkeeping_scan_runs_through_the_fragment_check() -> Result<(), String> {
        let root = fresh_root("changelog-internal-ci")?;
        fs::create_dir_all(root.join(FRAGMENT_DIR)).map_err(|err| err.to_string())?;
        fs::create_dir_all(root.join("docs")).map_err(|err| err.to_string())?;
        let policy: String = std::iter::once("## Sections\n\n".to_string())
            .chain(ALLOWED_SECTIONS.iter().map(|s| format!("- `{s}`\n")))
            .collect();
        fs::write(root.join(POLICY_DOC), policy).map_err(|err| err.to_string())?;
        fs::write(root.join(FRAGMENT_DIR).join(README), "# fragments\n")
            .map_err(|err| err.to_string())?;
        fs::write(
            root.join(FRAGMENT_DIR).join("corpus-batch.md"),
            "<!-- section: Added -->\n- Verdict corpus: 14 cases (#1).\n",
        )
        .map_err(|err| err.to_string())?;

        let violations = changelog_fragment_violations(&root, false);
        let _ = fs::remove_dir_all(&root);
        let violations = violations?;
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].contains("corpus or Repo ops bookkeeping"),
            "{violations:?}"
        );
        Ok(())
    }
}

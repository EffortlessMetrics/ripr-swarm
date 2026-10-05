//! `cargo xtask check-changelog-fragments` (#6341): every `changelog.d/`
//! fragment must fold into `CHANGELOG.md` without a reader having to guess.
//! A fragment with no section line, an unknown section, or no issue/PR
//! reference used to pass every gate and then drop out of the release notes
//! at the fold. `--release-cut` additionally fails on any fragment at all, so
//! the fold step is a command that fails rather than a checklist line.

use std::fs;
use std::path::Path;

use crate::{FixKind, PolicyReportSpec, finish_policy_report};

const FRAGMENT_DIR: &str = "changelog.d";
const README: &str = "README.md";
const POLICY_DOC: &str = "docs/CHANGELOG_POLICY.md";

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
];

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
            why_it_matters: "Fragments are folded into CHANGELOG.md by hand at the release cut. A fragment with no section, an unknown section, or no issue/PR reference cannot be placed or traced, and a fragment left unfolded drops a user-facing change from the release notes.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Start the fragment with `<!-- section: X -->`, where X is one of the sections in docs/CHANGELOG_POLICY.md.",
                "Write the entry as a `- ` bullet with the issue or PR reference (`#N` or a link) in it.",
                "Name the file `<pr-or-issue>-<short-slug>.md` in lowercase kebab case.",
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
    Ok(violations)
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
    match section_of(first) {
        Some(section) if ALLOWED_SECTIONS.contains(&section) => {}
        Some(section) => violations.push(format!(
            "{path}: unknown section `{section}`; use one of {}",
            ALLOWED_SECTIONS.join(", ")
        )),
        None => violations.push(format!(
            "{path}: first line must be `<!-- section: X -->`, found `{first}`"
        )),
    }

    let body: Vec<&str> = lines.collect();
    match body.iter().find(|line| !line.trim().is_empty()) {
        None => violations.push(format!("{path}: entry is empty")),
        Some(line) if !line.starts_with("- ") => violations.push(format!(
            "{path}: entry must start with a `- ` bullet at column 0, found `{line}`"
        )),
        Some(line) if line[2..].trim().is_empty() => {
            violations.push(format!("{path}: the bullet line has no entry text"))
        }
        Some(_) => {}
    }
    // HTML comments are copied into CHANGELOG.md verbatim at the fold, so a
    // second section line or a reference hidden in a comment is not an entry.
    if body
        .iter()
        .any(|line| line.trim_start().starts_with("<!--"))
    {
        violations.push(format!(
            "{path}: only the first line may be an HTML comment; one fragment holds one section"
        ));
    }
    if !body
        .iter()
        .filter(|line| !line.trim_start().starts_with("<!--"))
        .any(|line| has_issue_reference(line))
    {
        violations.push(format!(
            "{path}: entry names no issue or PR; add `(#N)` or a link to it"
        ));
    }
    violations
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
        assert_eq!(violations.len(), 3, "{violations:?}");
        assert!(violations[0].contains("first line must be `<!-- section: X -->`"));
        assert!(violations[1].contains("entry is empty"));
        assert!(violations[2].contains("names no issue or PR"));
    }

    #[test]
    fn unknown_section_is_named() {
        let text = "<!-- section: Improvements -->\n- Faster (#1).\n";
        let violations = fragment_violations("faster.md", text);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("unknown section `Improvements`"));
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
        assert!(violations[0].contains("only the first line may be an HTML comment"));
        let two_sections =
            "<!-- section: Fixed -->\n- x (#1).\n<!-- section: Added -->\n- y (#2).\n";
        let violations = fragment_violations("two.md", two_sections);
        assert_eq!(violations.len(), 1, "{violations:?}");
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
        // Six allowed sections are missing from the list (a `Docs` item under
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
}

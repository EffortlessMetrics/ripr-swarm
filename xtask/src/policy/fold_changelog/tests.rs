use super::{
    ADMISSION_FILE_CAP, FoldOptions, fold_checked, fold_in, fold_text,
    oversize_release_cut_warning, parse_args,
};
use crate::policy::changelog_fragments::changelog_fragment_violations;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static ROOTS: AtomicUsize = AtomicUsize::new(0);

const CHANGELOG: &str = "# Changelog\n\n## Unreleased\n\n### Changed\n\n- Old change (#1).\n\n### Fixed\n\n- Old fix (#2).\n  continued here.\n\n## 0.10.0\n\n### Fixed\n\n- Ancient (#3).\n";

fn frag(section: &str, body: &str) -> String {
    format!("<!-- section: {section} -->\n{body}")
}

fn pair(name: &str, section: &str, body: &str) -> (String, String) {
    (name.to_string(), frag(section, body))
}

fn root_with(fragments: &[(&str, String)]) -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-fold-{}-{}",
        std::process::id(),
        ROOTS.fetch_add(1, Ordering::Relaxed)
    ));
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(root.join("changelog.d")).map_err(|e| e.to_string())?;
    fs::write(root.join("CHANGELOG.md"), CHANGELOG).map_err(|e| e.to_string())?;
    fs::write(root.join("changelog.d/README.md"), "readme\n").map_err(|e| e.to_string())?;
    for (name, text) in fragments {
        fs::write(root.join("changelog.d").join(name), text).map_err(|e| e.to_string())?;
    }
    Ok(root)
}

fn opts(write: bool, limit: Option<usize>) -> FoldOptions {
    FoldOptions {
        write,
        limit,
        keep_fragments: false,
        release_cut: false,
    }
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn appends_to_first_matching_heading_with_continuation_lines() -> Result<(), String> {
    let fragments = [pair(
        "10-new-fix.md",
        "Fixed",
        "- New fix\n  with continuation (#10).\n",
    )];
    let (out, folded) = fold_text(CHANGELOG, &fragments)?;
    assert_eq!(folded, vec![("10-new-fix.md".into(), "Fixed".into(), 1)]);
    let expected = CHANGELOG.replace(
        "  continued here.\n",
        "  continued here.\n- New fix\n  with continuation (#10).\n",
    );
    assert_eq!(out, expected);
    // The versioned section below is untouched.
    assert!(out.ends_with("### Fixed\n\n- Ancient (#3).\n"));
    Ok(())
}

#[test]
fn missing_heading_is_added_at_end_of_unreleased_only() -> Result<(), String> {
    let fragments = [pair("5-ci-bits.md", "Internal / CI", "- CI bit (#5).\n")];
    let (out, _) = fold_text(CHANGELOG, &fragments)?;
    let expected = CHANGELOG.replace(
        "  continued here.\n\n## 0.10.0",
        "  continued here.\n\n### Internal / CI\n\n- CI bit (#5).\n\n## 0.10.0",
    );
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn fragments_fold_in_file_name_order_not_argument_order() -> Result<(), String> {
    let fragments = [
        pair("b-second.md", "Changed", "- B (#12).\n"),
        pair("a-first.md", "Changed", "- A (#11).\n"),
    ];
    // fold_text folds in the order given; fold_in sorts by name first.
    let root = root_with(&[
        ("b-second.md", fragments[0].1.clone()),
        ("a-first.md", fragments[1].1.clone()),
    ])?;
    let report = fold_in(&root, &opts(false, None))?;
    let names: Vec<&str> = report.folded.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["a-first.md", "b-second.md"]);
    let a = report.changelog.find("- A (#11).").ok_or("A missing")?;
    let b = report.changelog.find("- B (#12).").ok_or("B missing")?;
    let old = report.changelog.find("- Old change (#1).").ok_or("old")?;
    assert!(old < a && a < b);
    Ok(())
}

#[test]
fn dry_run_default_writes_nothing() -> Result<(), String> {
    let root = root_with(&[("a-one.md", frag("Fixed", "- One (#1).\n"))])?;
    let report = fold_in(&root, &opts(false, None))?;
    assert_eq!(report.folded.len(), 1);
    assert_eq!(
        fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?,
        CHANGELOG
    );
    assert!(root.join("changelog.d/a-one.md").exists());
    Ok(())
}

#[test]
fn write_is_idempotent_and_keeps_readme() -> Result<(), String> {
    let root = root_with(&[("a-one.md", frag("Fixed", "- One (#1).\n"))])?;
    fold_in(&root, &opts(true, None))?;
    let after = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
    assert_eq!(after.matches("- One (#1).").count(), 1);
    assert!(!root.join("changelog.d/a-one.md").exists());
    assert!(root.join("changelog.d/README.md").exists());
    let again = fold_in(&root, &opts(true, None))?;
    assert!(again.folded.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?,
        after
    );
    Ok(())
}

#[test]
fn already_folded_entry_is_refused_not_duplicated() -> Result<(), String> {
    let root = root_with(&[("a-one.md", frag("Fixed", "- One (#1).\n"))])?;
    let keep = FoldOptions {
        write: true,
        limit: None,
        keep_fragments: true,
        release_cut: false,
    };
    fold_in(&root, &keep)?;
    assert!(root.join("changelog.d/a-one.md").exists());
    let err = fold_in(&root, &opts(true, None))
        .err()
        .ok_or("expected refusal")?;
    assert!(
        err.contains("a-one.md") && err.contains("duplicates an entry in CHANGELOG.md Unreleased"),
        "{err}"
    );
    Ok(())
}

#[test]
fn mixed_already_folded_and_new_entries_are_refused_before_writing() -> Result<(), String> {
    let name = "b-mixed.md";
    let mixed = frag(
        "Changed",
        "- Old  fix (#2). continued here.\n- Genuinely new (#12).\n",
    );
    let fresh = frag("Fixed", "- Another new fix (#11).\n");
    let fragments = [("a-first.md", fresh), (name, mixed)];

    for flags in [
        &["--write", "--limit", "150"][..],
        &["--write", "--release-cut"],
    ] {
        let root = root_with(&fragments)?;
        let result = fold_in(&root, &parse_args(&args(flags))?);
        let changelog = fs::read(root.join("CHANGELOG.md"));
        let fragment_bytes: Vec<_> = fragments
            .iter()
            .map(|(name, _)| fs::read(root.join("changelog.d").join(name)))
            .collect();

        let err = result
            .err()
            .ok_or("expected mixed fragment to be refused")?;
        assert!(
            err.contains("changelog.d/b-mixed.md")
                && err.contains("- Old fix (#2). continued here.")
                && err.contains("duplicates an entry in CHANGELOG.md Unreleased"),
            "{flags:?}: {err}"
        );
        assert_eq!(changelog.map_err(|e| e.to_string())?, CHANGELOG.as_bytes());
        for (actual, (_, original)) in fragment_bytes.into_iter().zip(&fragments) {
            assert_eq!(actual.map_err(|e| e.to_string())?, original.as_bytes());
        }
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A fixture root the whole `check-changelog-fragments` check can read: the
/// repository's own policy doc beside the fixture changelog and fragments.
fn checked_root(changelog: &str, fragments: &[(&str, String)]) -> Result<PathBuf, String> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest has no repo parent")?
        .to_path_buf();
    let policy_doc = fs::read_to_string(repo_root.join("docs/CHANGELOG_POLICY.md"))
        .map_err(|e| e.to_string())?;
    let root = root_with(fragments)?;
    fs::create_dir_all(root.join("docs")).map_err(|e| e.to_string())?;
    fs::write(root.join("docs/CHANGELOG_POLICY.md"), policy_doc).map_err(|e| e.to_string())?;
    fs::write(root.join("CHANGELOG.md"), changelog).map_err(|e| e.to_string())?;
    Ok(root)
}

#[test]
fn a_tree_the_check_refuses_is_refused_before_any_write() -> Result<(), String> {
    // The fold itself has nothing to say about an Unreleased bullet with no
    // reference; only the whole check does, and before #15606 it ran after
    // the write on `--release-cut` and not at all on `--limit`.
    let untraceable = CHANGELOG.replace("- Old change (#1).", "- Old change.");
    assert_ne!(untraceable, CHANGELOG);
    let fragments = [("a-new.md", frag("Fixed", "- A new fix (#11).\n"))];

    for flags in [
        &["--write", "--limit", "149"][..],
        &["--write", "--release-cut"],
        &["--release-cut"],
        &[],
    ] {
        let root = checked_root(&untraceable, &fragments)?;
        let err = fold_checked(&root, &parse_args(&args(flags))?)
            .err()
            .ok_or_else(|| format!("{flags:?}: expected the fold to be refused"))?;
        assert!(
            err.starts_with("refusing to fold: check-changelog-fragments fails")
                && err.contains("nothing written")
                && err.contains("CHANGELOG.md: Unreleased entry `- Old change.` names no issue"),
            "{flags:?}: {err}"
        );
        assert_eq!(
            fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?,
            untraceable,
            "{flags:?}"
        );
        assert_eq!(
            fs::read_to_string(root.join("changelog.d/a-new.md")).map_err(|e| e.to_string())?,
            fragments[0].1,
            "{flags:?}"
        );
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    }

    // The unchecked fold accepts the same tree, so the refusal above is the
    // check's and not a side effect of the fixture.
    let root = checked_root(&untraceable, &fragments)?;
    let unchecked = fold_in(&root, &opts(true, None))?;
    assert_eq!(unchecked.folded.len(), 1);
    assert!(!root.join("changelog.d/a-new.md").exists());
    fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn a_tree_the_check_accepts_still_folds_on_both_write_routes() -> Result<(), String> {
    for flags in [
        &["--write", "--limit", "149"][..],
        &["--write", "--release-cut"],
    ] {
        let root = checked_root(
            CHANGELOG,
            &[("a-new.md", frag("Fixed", "- A new fix (#11).\n"))],
        )?;
        let report = fold_checked(&root, &parse_args(&args(flags))?)?;
        assert_eq!(
            report.folded,
            vec![("a-new.md".to_string(), "Fixed".to_string(), 1)],
            "{flags:?}"
        );
        let written = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
        assert_eq!(written, report.changelog, "{flags:?}");
        assert!(
            written.contains("  continued here.\n- A new fix (#11).\n"),
            "{written}"
        );
        assert!(!root.join("changelog.d/a-new.md").exists(), "{flags:?}");
        assert_eq!(
            changelog_fragment_violations(&root, true)?,
            Vec::<String>::new(),
            "{flags:?}"
        );
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn limit_takes_first_n_in_name_order() -> Result<(), String> {
    let root = root_with(&[
        ("a-one.md", frag("Fixed", "- One (#1).\n")),
        ("b-two.md", frag("Fixed", "- Two (#2).\n")),
        ("c-three.md", frag("Fixed", "- Three (#3).\n")),
    ])?;
    let report = fold_in(&root, &opts(true, Some(2)))?;
    assert_eq!(report.folded.len(), 2);
    assert_eq!(report.remaining, 1);
    assert!(!root.join("changelog.d/b-two.md").exists());
    assert!(root.join("changelog.d/c-three.md").exists());
    Ok(())
}

#[test]
fn unknown_section_is_refused_with_file_name_and_writes_nothing() -> Result<(), String> {
    let root = root_with(&[
        ("a-good.md", frag("Fixed", "- Good (#1).\n")),
        ("b-bad.md", frag("Features", "- Bad (#2).\n")),
    ])?;
    let err = fold_in(&root, &opts(true, None))
        .err()
        .ok_or("expected refusal")?;
    assert!(
        err.contains("b-bad.md") && err.contains("Features"),
        "{err}"
    );
    assert_eq!(
        fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?,
        CHANGELOG
    );
    assert!(root.join("changelog.d/a-good.md").exists());
    Ok(())
}

#[test]
fn missing_marker_and_reference_are_refused_by_name() {
    let no_marker = [("c-nomarker.md".to_string(), "- Entry (#1).\n".to_string())];
    let err = fold_text(CHANGELOG, &no_marker).err().unwrap_or_default();
    assert!(
        err.contains("c-nomarker.md") && err.contains("section"),
        "{err}"
    );
    let no_ref = [pair("d-noref.md", "Fixed", "- Entry with no reference.\n")];
    let err = fold_text(CHANGELOG, &no_ref).err().unwrap_or_default();
    assert!(err.contains("d-noref.md"), "{err}");
}

#[test]
fn missing_unreleased_heading_is_refused() {
    let fragments = [pair("a-one.md", "Fixed", "- One (#1).\n")];
    let err = fold_text("# Changelog\n\n## 0.10.0\n", &fragments)
        .err()
        .unwrap_or_default();
    assert!(err.contains("Unreleased"), "{err}");
}

#[test]
fn argument_parsing_is_strict() -> Result<(), String> {
    assert_eq!(parse_args(&[])?, opts(false, None));
    assert_eq!(
        parse_args(&args(&["--write", "--limit", "150"]))?,
        opts(true, Some(150))
    );
    assert_eq!(parse_args(&args(&["--limit=3"]))?, opts(false, Some(3)));
    for bad in [
        args(&["--write", "--dry-run"]),
        args(&["--limit"]),
        args(&["--limit", "0"]),
        args(&["--limit", "x"]),
        args(&["--bogus"]),
    ] {
        assert!(parse_args(&bad).is_err(), "{bad:?}");
    }
    Ok(())
}

#[test]
fn release_cut_flag_parses_and_conflicts_with_batch_flags() -> Result<(), String> {
    let cut = parse_args(&args(&["--write", "--release-cut"]))?;
    assert!(cut.release_cut && cut.write && cut.limit.is_none());
    let plan = parse_args(&args(&["--release-cut"]))?;
    assert!(plan.release_cut && !plan.write);
    for clash in [["--limit", "5"], ["--keep-fragments", ""]] {
        let mut argv = vec!["--write", "--release-cut"];
        argv.extend(clash.iter().filter(|a| !a.is_empty()));
        let err = parse_args(&args(&argv)).err().ok_or("expected refusal")?;
        assert!(
            err.contains("--release-cut") && err.contains("conflicts"),
            "{err}"
        );
    }
    Ok(())
}

#[test]
fn release_cut_fold_leaves_no_fragment_for_the_cut_check() -> Result<(), String> {
    let root = root_with(&[
        ("a-one.md", frag("Fixed", "- One (#1).\n")),
        ("b-two.md", frag("Docs", "- Two (#2).\n")),
    ])?;
    let cut = FoldOptions {
        write: true,
        limit: None,
        keep_fragments: false,
        release_cut: true,
    };
    let report = fold_in(&root, &cut)?;
    assert_eq!(report.folded.len(), 2);
    assert_eq!(report.remaining, 0);
    let left: Vec<_> = fs::read_dir(root.join("changelog.d"))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec!["README.md".to_string()]);
    Ok(())
}

#[test]
fn oversize_release_cut_warning_advises_fold_and_delete_batches_not_keep_fragments()
-> Result<(), String> {
    assert_eq!(oversize_release_cut_warning(ADMISSION_FILE_CAP - 1), None);
    let warning = oversize_release_cut_warning(ADMISSION_FILE_CAP)
        .ok_or("expected oversize warning when folded fragments + CHANGELOG.md exceed cap")?;
    assert!(warning.contains("161 changed files"), "{warning}");
    assert!(warning.contains("--write --limit 149"), "{warning}");
    assert!(warning.contains("changelog.d/README.md"), "{warning}");
    assert!(!warning.contains("--keep-fragments"), "{warning}");
    assert!(!warning.contains("snapshot"), "{warning}");
    Ok(())
}

#[test]
fn kept_fragments_fail_ordinary_validation_while_fold_and_delete_batch_passes() -> Result<(), String>
{
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest has no repo parent")?
        .to_path_buf();
    let policy_doc = fs::read_to_string(repo_root.join("docs/CHANGELOG_POLICY.md"))
        .map_err(|e| e.to_string())?;

    let kept_root = root_with(&[("a-one.md", frag("Fixed", "- One (#1).\n"))])?;
    fs::create_dir_all(kept_root.join("docs")).map_err(|e| e.to_string())?;
    fs::write(kept_root.join("docs/CHANGELOG_POLICY.md"), &policy_doc)
        .map_err(|e| e.to_string())?;
    let keep = FoldOptions {
        write: true,
        limit: None,
        keep_fragments: true,
        release_cut: false,
    };
    fold_in(&kept_root, &keep)?;
    let kept_violations = changelog_fragment_violations(&kept_root, false)?;
    assert!(
        kept_violations.iter().any(|v| v.contains("a-one.md")
            && v.contains("duplicates an entry in CHANGELOG.md Unreleased")),
        "expected duplicate-entry violation for kept fragment, got {kept_violations:?}"
    );
    let _ = fs::remove_dir_all(&kept_root);

    let batch_root = root_with(&[
        ("a-one.md", frag("Fixed", "- One (#1).\n")),
        ("b-two.md", frag("Fixed", "- Two (#2).\n")),
    ])?;
    fs::create_dir_all(batch_root.join("docs")).map_err(|e| e.to_string())?;
    fs::write(batch_root.join("docs/CHANGELOG_POLICY.md"), &policy_doc)
        .map_err(|e| e.to_string())?;
    let report = fold_in(&batch_root, &opts(true, Some(1)))?;
    assert_eq!(report.folded.len(), 1);
    assert_eq!(report.remaining, 1);
    let batch_violations = changelog_fragment_violations(&batch_root, false)?;
    assert!(
        batch_violations.is_empty(),
        "expected fold-and-delete batch to pass check-changelog-fragments, got {batch_violations:?}"
    );
    let _ = fs::remove_dir_all(&batch_root);
    Ok(())
}

#[test]
fn fold_changelog_help_and_readme_align_on_batched_fold_and_delete() -> Result<(), String> {
    let catalog = crate::command::command_catalog();
    let entry = catalog
        .iter()
        .find(|entry| entry.command.starts_with("fold-changelog "))
        .ok_or("fold-changelog missing from command_catalog")?;
    assert!(
        entry.notes.contains("local inspection only"),
        "help notes should restrict --keep-fragments to local inspection: {}",
        entry.notes
    );
    assert!(
        entry.notes.contains("149 fragments"),
        "help notes should document the 149-fragment batch limit: {}",
        entry.notes
    );
    assert!(
        !entry.notes.contains("snapshot"),
        "help notes must not advise a kept-fragment snapshot: {}",
        entry.notes
    );

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest has no repo parent")?
        .to_path_buf();
    let readme =
        fs::read_to_string(repo_root.join("changelog.d/README.md")).map_err(|e| e.to_string())?;
    assert!(
        readme.contains("--write --limit 149"),
        "changelog.d/README.md should document --write --limit 149"
    );
    assert!(
        readme.contains("local inspection only"),
        "changelog.d/README.md should restrict --keep-fragments to local inspection"
    );
    assert!(
        !readme.contains("deletion-only batch PRs"),
        "changelog.d/README.md must not advise kept-fragment deletion-only batch PRs"
    );
    assert!(
        !readme.contains("fold a snapshot ahead of the cut"),
        "changelog.d/README.md must not advise folding a snapshot ahead of the cut"
    );
    Ok(())
}

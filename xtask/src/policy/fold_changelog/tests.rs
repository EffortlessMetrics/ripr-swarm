use super::{FoldOptions, fold_in, fold_text, parse_args};
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
    assert!(err.contains("a-one.md") && err.contains("already"), "{err}");
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

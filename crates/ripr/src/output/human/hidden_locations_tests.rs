//! RIPR-SPEC-0122 / #8139: Hidden rows share finding-location presentation.

use super::render;
use super::tests::{bounded_output_with_findings, sample_finding};
use crate::domain::{SourceCurrentness, SourceLocation};
use std::path::PathBuf;

fn assert_locations(root: PathBuf, currentness: SourceCurrentness) {
    let findings = (0..3)
        .map(|index| {
            let mut finding = sample_finding();
            finding.id = format!("finding-{index}");
            finding.probe.location =
                SourceLocation::new(root.join(format!("src/f{index}.rs")), 7 + index, 1);
            finding.source_currentness = currentness;
            finding
        })
        .collect();
    let mut output = bounded_output_with_findings(findings);
    output.root = root;
    assert_eq!(output.findings.len(), 3);
    let rendered = render(&output);
    let selected = currentness == SourceCurrentness::CandidateCurrent;
    assert_eq!(
        rendered.contains("  File: ./src/f0.rs:7\n"),
        selected,
        "{rendered}"
    );
    assert!(rendered.contains("\nHidden:\n"), "{rendered}");
    for index in usize::from(selected)..3 {
        assert!(
            rendered.contains(&format!(
                "    - ./src/f{index}.rs:{} (weakly_exposed)\n",
                7 + index
            )),
            "{rendered}"
        );
    }
    assert!(rendered.contains("Summary: 3 probe(s)"), "{rendered}");
    assert_eq!(
        rendered.matches("    - ./src/").count(),
        3 - usize::from(selected)
    );
    assert!(rendered.contains("  Full evidence: rerun with --format human-full\n"));
    assert!(rendered.contains("  Machine data: rerun with --format json\n"));
}

#[test]
fn absolute_root_selected() -> std::io::Result<()> {
    assert_locations(
        std::env::current_dir()?.join("location-root"),
        SourceCurrentness::CandidateCurrent,
    );
    Ok(())
}

#[test]
fn absolute_root_without_selection() -> std::io::Result<()> {
    for currentness in [
        SourceCurrentness::BaseDeleted,
        SourceCurrentness::UnresolvedSubject,
    ] {
        assert_locations(std::env::current_dir()?.join("location-root"), currentness);
    }
    Ok(())
}

#[test]
fn relative_subdirectory_root_selected() {
    assert_locations("nested/root".into(), SourceCurrentness::CandidateCurrent);
}

#[test]
fn relative_subdirectory_root_without_selection() {
    for currentness in [
        SourceCurrentness::BaseDeleted,
        SourceCurrentness::UnresolvedSubject,
    ] {
        assert_locations("nested/root".into(), currentness);
    }
}

#[test]
fn dot_root_keeps_existing_locations() {
    for currentness in [
        SourceCurrentness::CandidateCurrent,
        SourceCurrentness::BaseDeleted,
        SourceCurrentness::UnresolvedSubject,
    ] {
        assert_locations(".".into(), currentness);
    }
}

#[test]
fn outside_parent_escape_and_unrooted_paths_keep_their_spelling() -> std::io::Result<()> {
    let root = std::env::current_dir()?.join("location-root");
    let files = [
        root.with_file_name("outside.rs"),
        root.join("../escaped.rs"),
        PathBuf::from("src/relative.rs"),
    ];
    let findings = files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let mut finding = sample_finding();
            finding.id = format!("finding-{index}");
            finding.probe.location = SourceLocation::new(file, 7, 1);
            finding.source_currentness = SourceCurrentness::BaseDeleted;
            finding
        })
        .collect();
    let mut output = bounded_output_with_findings(findings);
    output.root = root;
    let rendered = render(&output);
    assert!(rendered.contains("(no_actionable_gap)"), "{rendered}");
    for file in files {
        assert!(
            rendered.contains(&format!(
                "    - {}:7 (weakly_exposed)\n",
                crate::output::path::display_path(&file)
            )),
            "{rendered}"
        );
    }
    Ok(())
}

#[test]
fn path_encoding_and_terminal_escaping_are_preserved() {
    let root = PathBuf::from("nested/root");
    let mut finding = sample_finding();
    finding.probe.location = SourceLocation::new(root.join("src/percent%name\u{1b}.rs"), 7, 1);
    finding.source_currentness = SourceCurrentness::BaseDeleted;
    let mut output = bounded_output_with_findings(vec![finding]);
    output.root = root;
    let rendered = render(&output);
    assert!(
        rendered.contains("    - ./src/percent%25name\\u{1b}.rs:7 (weakly_exposed)\n"),
        "{rendered}"
    );
    assert!(!rendered.contains('\u{1b}'), "{rendered}");
}

#[test]
fn location_change_keeps_the_twenty_row_cap_and_continuation() {
    let root = PathBuf::from("nested/root");
    let findings = (0..23)
        .map(|index| {
            let mut finding = sample_finding();
            finding.id = format!("finding-{index}");
            finding.probe.location =
                SourceLocation::new(root.join(format!("src/f{index:02}.rs")), index + 1, 1);
            finding.source_currentness = SourceCurrentness::BaseDeleted;
            finding
        })
        .collect();
    let mut output = bounded_output_with_findings(findings);
    output.root = root;
    let rendered = render(&output);
    assert!(
        rendered.contains("All 23 finding(s) are base-side evidence"),
        "{rendered}"
    );
    assert_eq!(rendered.matches("    - ./src/").count(), 20, "{rendered}");
    assert!(
        rendered.contains("    - ./src/f00.rs:1 (weakly_exposed)\n"),
        "{rendered}"
    );
    assert!(
        rendered.contains("    - ./src/f19.rs:20 (weakly_exposed)\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("    - ./src/f20.rs:"), "{rendered}");
    assert!(rendered.contains("    - … and 3 more omitted finding(s); every identity is in --format json with RIPR_CHECK_FINDINGS_BYTES=0.\n"), "{rendered}");
}

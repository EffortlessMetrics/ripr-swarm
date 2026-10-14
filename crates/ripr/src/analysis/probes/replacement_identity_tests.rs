//! #7807: an exact replacement consumes only the old subject it represents.

use super::SeededProbe;
use super::diff::probes_for_file_with_relations;
use crate::analysis::diff::parse_unified_diff;
use crate::analysis::facts::OwnedRustIndex;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::ProbeFamily;
use crate::testing::fixture_git::{fixture_git_ok, fixture_git_stdout, remove_fixture_tree};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn git_probes(before: &str, after: &str) -> Result<Vec<SeededProbe>, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-replacement-{stamp}"));
    let result: Result<Vec<SeededProbe>, String> = (|| {
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), before).map_err(|error| error.to_string())?;
        fixture_git_ok(&root, &["init", "-q"])?;
        fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(&root, &["add", "src/lib.rs"])?;
        fixture_git_ok(
            &root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "before",
            ],
        )?;
        std::fs::write(root.join("src/lib.rs"), after).map_err(|error| error.to_string())?;
        let patch = fixture_git_stdout(&root, &["diff", "--unified=0", "HEAD", "--"])?;
        assert!(patch.contains("@@"), "nonempty real Git subject: {patch}");
        let files = parse_unified_diff(&patch);
        assert_eq!(files.len(), 1, "{patch}");
        let changed = files.first().ok_or("missing changed file")?;
        assert_eq!(changed.path, PathBuf::from("src/lib.rs"));
        assert!(!changed.removed_lines.is_empty(), "{patch}");
        let facts = RaRustSyntaxAdapter.summarize_file(&changed.path, after)?;
        assert!(!facts.used_lexical_fallback);
        let index = RustIndex::from_owned(OwnedRustIndex {
            files: BTreeMap::from([(changed.path.clone(), facts)]),
            ..Default::default()
        });
        Ok(probes_for_file_with_relations(&root, changed, &index, &[]))
    })();
    let cleanup = remove_fixture_tree(&root);
    let probes = result?;
    cleanup?;
    Ok(probes)
}

fn historical(probes: &[SeededProbe], family: ProbeFamily) -> Vec<&str> {
    probes
        .iter()
        .filter(|seed| seed.probe.family == family && seed.probe.after.is_none())
        .map(|seed| seed.probe.expression.as_str())
        .collect()
}

#[test]
fn short_predicate_replacement_has_one_subject_at_exact_parser_bytes() -> Result<(), String> {
    let probes = git_probes(
        "pub fn f(a: u8, b: u8) -> bool {\n    a > b\n}\n",
        "pub fn f(a: u8, b: u8) -> bool {\n    a >= b\n}\n",
    )?;
    let predicates = probes
        .iter()
        .filter(|seed| seed.probe.family == ProbeFamily::Predicate)
        .collect::<Vec<_>>();
    assert_eq!(predicates.len(), 1, "{probes:?}");
    let seed = predicates.first().ok_or("missing predicate")?;
    assert_eq!(seed.probe.location.line, 2);
    assert_eq!(seed.probe.expression, "a >= b");
    assert_eq!(seed.probe.before.as_deref(), Some("a > b"));
    assert_eq!(seed.parser_span.map(|span| span.start_byte), Some(37));
    // Returning a predicate still carries its distinct ReturnValue family.
    assert!(
        probes
            .iter()
            .any(|seed| seed.probe.family == ProbeFamily::ReturnValue
                && seed.probe.after.as_deref() == Some("a >= b")),
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn short_constructor_replacement_keeps_its_exact_before_after_identity() -> Result<(), String> {
    let probes = git_probes(
        "pub fn f() -> Option<u8> {\n    Some(1)\n}\n",
        "pub fn f() -> Option<u8> {\n    Some(2)\n}\n",
    )?;
    assert!(
        historical(&probes, ProbeFamily::ReturnValue).is_empty(),
        "{probes:?}"
    );
    let values = probes
        .iter()
        .filter(|seed| seed.probe.family == ProbeFamily::ReturnValue)
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 1, "{probes:?}");
    let seed = values.first().ok_or("missing return")?;
    assert_eq!(seed.probe.location.line, 2);
    assert_eq!(seed.probe.before.as_deref(), Some("Some(1)"));
    assert_eq!(seed.probe.after.as_deref(), Some("Some(2)"));
    assert_eq!(seed.parser_span.map(|span| span.start_byte), Some(31));
    Ok(())
}

#[test]
fn a_replacement_without_the_old_family_keeps_deleted_error_behavior() -> Result<(), String> {
    let probes = git_probes(
        "pub fn f(a: u8) -> Result<u8, u8> {\n    Err(a)\n}\n",
        "pub fn f(a: u8) -> Result<u8, u8> {\n    Ok(a)\n}\n",
    )?;
    assert_eq!(
        historical(&probes, ProbeFamily::ErrorPath),
        vec!["Err(a)"],
        "{probes:?}"
    );
    let values = probes
        .iter()
        .filter(|seed| seed.probe.family == ProbeFamily::ReturnValue)
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 1, "{probes:?}");
    let seed = values.first().ok_or("missing Ok constructor")?;
    assert_eq!(seed.probe.after.as_deref(), Some("Ok(a)"));
    assert_eq!(seed.parser_span.map(|span| span.start_byte), Some(40));
    Ok(())
}

#[test]
fn one_constructor_span_cannot_consume_two_old_constructor_edits() -> Result<(), String> {
    let probes = git_probes(
        "pub fn f() -> (Option<u8>, Option<u8>) {\n    let _ = (Some(1), Some(2));\n    (None, None)\n}\n",
        "pub fn f() -> (Option<u8>, Option<u8>) {\n    let _ = (Some(3), Some(4));\n    (None, None)\n}\n",
    )?;
    assert_eq!(
        historical(&probes, ProbeFamily::ReturnValue),
        vec!["let _ = (Some(1), Some(2));"],
        "{probes:?}"
    );
    // This bounded repair does not change the parser's one-shape-per-family selector.
    assert!(
        probes
            .iter()
            .any(|seed| seed.probe.after.as_deref() == Some("Some(3)")
                && seed.parser_span.map(|span| span.start_byte) == Some(54)),
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn unequal_replacement_and_wrapper_only_return_keep_distinct_obligations() -> Result<(), String> {
    let unequal = git_probes(
        "pub fn f(a: u8, b: u8) {\n    if a > b {}\n    if a < b {}\n}\n",
        "pub fn f(a: u8, b: u8) {\n    if a >= b {}\n}\n",
    )?;
    assert_eq!(
        historical(&unequal, ProbeFamily::Predicate),
        vec!["if a > b {}", "if a < b {}"],
        "{unequal:?}"
    );
    let wrapper = git_probes(
        "pub fn f(a: u8) -> Result<(), u8> {\n    let _ =\n        Err(a);\n    Ok(())\n}\n",
        "pub fn f(a: u8) -> Result<(), u8> {\n    return\n        Err(a);\n    Ok(())\n}\n",
    )?;
    for family in [ProbeFamily::ErrorPath, ProbeFamily::ReturnValue] {
        assert!(
            wrapper.iter().any(|seed| seed.probe.family == family
                && seed.probe.location.line == 2
                && seed.probe.after.as_deref() == Some("return\n        Err(a)")),
            "{wrapper:?}"
        );
    }
    Ok(())
}

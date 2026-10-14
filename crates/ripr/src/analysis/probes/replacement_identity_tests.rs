//! #7807: an exact replacement consumes only the old subject it represents.

use super::SeededProbe;
use super::diff::{probes_for_file_with_relations, resolve_probe_source_currentness};
use crate::analysis::diff::{ChangedFile, parse_unified_diff};
use crate::analysis::facts::OwnedRustIndex;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::ProbeFamily;
use crate::testing::fixture_git::{fixture_git_ok, fixture_git_stdout, remove_fixture_tree};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn git_probes(before: &str, after: &str) -> Result<Vec<SeededProbe>, String> {
    Ok(git_probe_fixture(&[("src/lib.rs", before, after)])?.probes)
}

struct GitProbeFixture {
    root: PathBuf,
    patch: String,
    files: Vec<ChangedFile>,
    probes: Vec<SeededProbe>,
}

fn git_probe_fixture(sources: &[(&str, &str, &str)]) -> Result<GitProbeFixture, String> {
    assert!(!sources.is_empty(), "a real Git fixture needs source files");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-replacement-{stamp}"));
    let result: Result<GitProbeFixture, String> = (|| {
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        for (path, before, _) in sources {
            std::fs::write(root.join(path), before).map_err(|error| error.to_string())?;
        }
        fixture_git_ok(&root, &["init", "-q"])?;
        fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(&root, &["add", "src"])?;
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
        for (path, _, after) in sources {
            std::fs::write(root.join(path), after).map_err(|error| error.to_string())?;
        }
        let patch = fixture_git_stdout(&root, &["diff", "--unified=0", "HEAD", "--"])?;
        assert!(patch.contains("@@"), "nonempty real Git subject: {patch}");
        let files = parse_unified_diff(&patch);
        assert_eq!(files.len(), sources.len(), "{patch}");
        let mut facts_by_path = BTreeMap::new();
        for (path, _, after) in sources {
            let path = PathBuf::from(*path);
            let changed = files
                .iter()
                .find(|changed| changed.path == path)
                .ok_or_else(|| format!("missing changed file {path:?}: {patch}"))?;
            assert!(!changed.removed_lines.is_empty(), "{patch}");
            let facts = RaRustSyntaxAdapter.summarize_file(&path, after)?;
            assert!(!facts.used_lexical_fallback);
            facts_by_path.insert(path, facts);
        }
        let index = RustIndex::from_owned(OwnedRustIndex {
            files: facts_by_path,
            ..Default::default()
        });
        let probes = files
            .iter()
            .flat_map(|changed| probes_for_file_with_relations(&root, changed, &index, &[]))
            .collect();
        Ok(GitProbeFixture {
            root: root.clone(),
            patch,
            files,
            probes,
        })
    })();
    let cleanup = remove_fixture_tree(&root);
    let probes = result?;
    cleanup?;
    Ok(probes)
}

#[derive(Debug, PartialEq, Eq)]
struct LiteralIdentity<'a> {
    file: &'a str,
    line: usize,
    family: &'static str,
    expression: &'a str,
    before: Option<&'a str>,
    after: Option<&'a str>,
    start_byte: Option<usize>,
    currentness: &'static str,
}

fn identities(
    fixture: &GitProbeFixture,
    candidate_side: bool,
) -> Result<Vec<LiteralIdentity<'_>>, String> {
    let mut identities = fixture
        .probes
        .iter()
        .filter(|seed| seed.probe.after.is_some() == candidate_side)
        .map(|seed| {
            let probe = &seed.probe;
            let file = probe
                .location
                .file
                .strip_prefix(&fixture.root)
                .map_err(|error| error.to_string())?
                .to_str()
                .ok_or("fixture path is not UTF-8")?;
            let changed = fixture
                .files
                .iter()
                .find(|changed| probe.location.file == fixture.root.join(&changed.path))
                .ok_or("probe has no changed file")?;
            Ok(LiteralIdentity {
                file,
                line: probe.location.line,
                family: probe.family.as_str(),
                expression: &probe.expression,
                before: probe.before.as_deref(),
                after: probe.after.as_deref(),
                start_byte: seed.parser_span.map(|span| span.start_byte),
                currentness: resolve_probe_source_currentness(changed, probe).as_str(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    identities.sort_by_key(|identity| (identity.file, identity.line, identity.family));
    Ok(identities)
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
        "pub fn f() -> (Option<u8>, Option<u8>) {\n    let t = (Some(1), Some(2));\n    (None, None)\n}\n",
        "pub fn f() -> (Option<u8>, Option<u8>) {\n    let t = (Some(3), Some(4));\n    (None, None)\n}\n",
    )?;
    assert_eq!(
        historical(&probes, ProbeFamily::ReturnValue),
        vec!["let t = (Some(1), Some(2));"],
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
        "pub fn f(a: u8) -> Result<(), u8> {\n    let _ =\n        Result::<(), u8>::Err(a);\n    Ok(())\n}\n",
        "pub fn f(a: u8) -> Result<(), u8> {\n    return\n        Result::<(), u8>::Err(a);\n    Ok(())\n}\n",
    )?;
    for family in [ProbeFamily::ErrorPath, ProbeFamily::ReturnValue] {
        assert!(
            wrapper.iter().any(|seed| seed.probe.family == family
                && seed.probe.location.line == 2
                && seed.probe.after.as_deref() == Some("return\n        Result::<(), u8>::Err(a)")),
            "{wrapper:?}"
        );
    }
    Ok(())
}

#[test]
fn guard_and_short_replacements_keep_file_local_line_and_byte_identities() -> Result<(), String> {
    let fixture = git_probe_fixture(&[
        (
            "src/a.rs",
            "pub fn f(n: i32) -> i8 {\n    match n {\n        n if n < 0 => -1,\n        _ => 1,\n    }\n}\n",
            "// shifted é\npub fn f(n: i32) -> i8 {\n    match n {\n        n if 0 > n => -1,\n        _ => 1,\n    }\n}\n",
        ),
        (
            "src/b.rs",
            "pub fn f(a: u8, b: u8) -> bool {\n    a > b\n}\n",
            "pub fn f(a: u8, b: u8) -> bool {\n    a >= b\n}\n",
        ),
        (
            "src/c.rs",
            "pub fn f() -> Option<u8> {\n    Some(1)\n}\n",
            "pub fn f() -> Option<u8> {\n    Some(2)\n}\n",
        ),
    ])?;
    assert_eq!(
        fixture
            .files
            .iter()
            .map(|file| file.path.as_path())
            .collect::<Vec<_>>(),
        vec![
            std::path::Path::new("src/a.rs"),
            std::path::Path::new("src/b.rs"),
            std::path::Path::new("src/c.rs"),
        ]
    );
    assert_eq!(
        fixture
            .files
            .iter()
            .map(|file| {
                file.replacement_counterparts()
                    .iter()
                    .map(|(new, old)| (*new, old.line))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        vec![vec![(4, 3)], vec![(2, 2)], vec![(2, 2)]]
    );
    // All current probes are pinned, including the separate return family.
    // The UTF-8 comment shifts parser bytes as well as source lines in a.rs.
    assert_eq!(
        identities(&fixture, true)?,
        vec![
            LiteralIdentity {
                file: "src/a.rs",
                line: 4,
                family: "predicate",
                expression: "0 > n",
                before: Some("n < 0"),
                after: Some("0 > n"),
                start_byte: Some(66),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/b.rs",
                line: 2,
                family: "predicate",
                expression: "a >= b",
                before: Some("a > b"),
                after: Some("a >= b"),
                start_byte: Some(37),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/b.rs",
                line: 2,
                family: "return_value",
                expression: "a >= b",
                before: Some("a > b"),
                after: Some("a >= b"),
                start_byte: Some(37),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/c.rs",
                line: 2,
                family: "return_value",
                expression: "Some(2)",
                before: Some("Some(1)"),
                after: Some("Some(2)"),
                start_byte: Some(31),
                currentness: "candidate_current",
            },
        ]
    );
    assert_eq!(identities(&fixture, false)?, vec![]);
    Ok(())
}

#[test]
fn guard_and_constructor_body_edits_keep_every_unrepresented_old_family() -> Result<(), String> {
    let fixture = git_probe_fixture(&[(
        "src/lib.rs",
        "pub fn f(n: i32) -> Option<u8> {\n    match n {\n        n if n < 0 => Some(1),\n        _ => None,\n    }\n}\n",
        "pub fn f(n: i32) -> Option<u8> {\n    match n {\n        n if 0 > n => Some(2),\n        _ => None,\n    }\n}\n",
    )])?;
    assert_eq!(
        fixture.files[0]
            .replacement_counterparts()
            .iter()
            .map(|(new, old)| (*new, old.line))
            .collect::<Vec<_>>(),
        vec![(3, 3)]
    );
    // No smaller subject carries both edits. A whole old before beside a
    // smaller after must not consume the predicate, arm or constructor history.
    assert_eq!(
        identities(&fixture, true)?,
        vec![
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "match_arm",
                expression: "n if 0 > n =>",
                before: Some("n if n < 0 => Some(1),"),
                after: Some("n if 0 > n =>"),
                // MatchArm's parser identity is the => token, not the text's byte 55.
                start_byte: Some(66),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "predicate",
                expression: "0 > n",
                before: Some("n if n < 0 => Some(1),"),
                after: Some("0 > n"),
                start_byte: Some(60),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "return_value",
                expression: "Some(2)",
                before: Some("n if n < 0 => Some(1),"),
                after: Some("Some(2)"),
                start_byte: Some(69),
                currentness: "candidate_current",
            },
        ]
    );
    assert_eq!(
        identities(&fixture, false)?,
        vec![
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "match_arm",
                expression: "n if n < 0 => Some(1),",
                before: Some("n if n < 0 => Some(1),"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "predicate",
                expression: "n if n < 0 => Some(1),",
                before: Some("n if n < 0 => Some(1),"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "return_value",
                expression: "n if n < 0 => Some(1),",
                before: Some("n if n < 0 => Some(1),"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
        ]
    );
    Ok(())
}

#[test]
fn a_guard_replacement_does_not_consume_an_identical_unpaired_arm() -> Result<(), String> {
    let fixture = git_probe_fixture(&[(
        "src/lib.rs",
        "/// The sign of `n`.\npub fn sign(n: i64) -> i8 {\n    match n {\n        n if n < 0 => -1,\n        0 => 0,\n        _ => 1,\n    }\n}\n\n/// Second.\npub fn sign2(n: i64) -> i8 {\n    match n {\n        n if n < 0 => -1,\n        0 => 0,\n        _ => 1,\n    }\n}\n",
        "/// The sign of `n`.\npub fn sign(n: i64) -> i8 {\n    match n {\n        n if 0 > n => -1,\n        0 => 0,\n        _ => 1,\n    }\n}\n\n/// Second.\npub fn sign2(n: i64) -> i8 {\n    match n {\n        0 => 0,\n        _ => 1,\n    }\n}\n",
    )])?;
    let changed = &fixture.files[0];
    assert_eq!(
        fixture
            .patch
            .lines()
            .filter_map(|line| line.strip_prefix("@@")?.split_once("@@"))
            .map(|(range, _)| range.trim())
            .collect::<Vec<_>>(),
        vec!["-4 +4", "-13 +12,0"]
    );
    // Equal text is not equal identity: old 4 has a counterpart; old 13 does not.
    // A zero-length new range anchors the deletion at the preceding new line12.
    assert_eq!(
        changed
            .removed_lines
            .iter()
            .map(|line| (line.line, line.new_side_line, line.text.trim()))
            .collect::<Vec<_>>(),
        vec![(4, 4, "n if n < 0 => -1,"), (13, 12, "n if n < 0 => -1,")]
    );
    assert_eq!(
        changed
            .replacement_counterparts()
            .iter()
            .map(|(new, old)| (*new, old.line))
            .collect::<Vec<_>>(),
        vec![(4, 4)]
    );
    assert_eq!(
        identities(&fixture, true)?,
        vec![LiteralIdentity {
            file: "src/lib.rs",
            line: 4,
            family: "predicate",
            expression: "0 > n",
            before: Some("n < 0"),
            after: Some("0 > n"),
            start_byte: Some(76),
            currentness: "candidate_current",
        }]
    );
    // Historical locations use the projected new-side coordinate; the real
    // old-side identity was pinned above, independently of that projection.
    assert_eq!(
        identities(&fixture, false)?,
        vec![
            LiteralIdentity {
                file: "src/lib.rs",
                line: 12,
                family: "match_arm",
                expression: "n if n < 0 => -1,",
                before: Some("n if n < 0 => -1,"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
            LiteralIdentity {
                file: "src/lib.rs",
                line: 12,
                family: "predicate",
                expression: "n if n < 0 => -1,",
                before: Some("n if n < 0 => -1,"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
        ]
    );
    Ok(())
}

#[test]
fn equal_two_line_short_replacements_keep_exact_counterparts() -> Result<(), String> {
    let fixture = git_probe_fixture(&[(
        "src/lib.rs",
        "pub fn f(a: u8, b: u8) {\n    if a > b {}\n    if a < b {}\n}\n",
        "pub fn f(a: u8, b: u8) {\n    if a >= b {}\n    if a <= b {}\n}\n",
    )])?;
    let changed = &fixture.files[0];
    assert_eq!(
        changed
            .removed_lines
            .iter()
            .map(|line| (line.line, line.new_side_line))
            .collect::<Vec<_>>(),
        vec![(2, 2), (3, 2)]
    );
    assert_eq!(
        changed
            .replacement_counterparts()
            .iter()
            .map(|(new, old)| (*new, old.line))
            .collect::<Vec<_>>(),
        vec![(2, 2), (3, 3)]
    );
    // Both lines have the same short-name set. Positional identity distinguishes
    // these separate edits.
    assert_eq!(
        identities(&fixture, true)?,
        vec![
            LiteralIdentity {
                file: "src/lib.rs",
                line: 2,
                family: "predicate",
                expression: "a >= b",
                before: Some("a > b"),
                after: Some("a >= b"),
                start_byte: Some(32),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/lib.rs",
                line: 3,
                family: "predicate",
                expression: "a <= b",
                before: Some("a < b"),
                after: Some("a <= b"),
                start_byte: Some(49),
                currentness: "candidate_current",
            },
        ]
    );
    assert_eq!(identities(&fixture, false)?, vec![]);
    Ok(())
}

#[test]
fn pure_short_conjunct_edits_keep_exact_current_and_historical_identities() -> Result<(), String> {
    // Every token on the changed arm is at most two bytes: the short-name
    // reconciliation, not the long-token fallback, must own these replacements.
    let old_chain = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if m > 0 && n <= 0 => -1,\n        _ => 1,\n    }\n}\n";
    let old_single = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if n <= 0 => -1,\n        _ => 1,\n    }\n}\n";
    let new_chain = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if m > 0 && n < 0 => -1,\n        _ => 1,\n    }\n}\n";
    let shifted_single = "// é\npub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        n if n < 0 => -1,\n        _ => 1,\n    }\n}\n";
    let deleted = "pub fn f(n: i32, m: i32) -> i8 {\n    match n {\n        _ => 1,\n    }\n}\n";
    let fixture = git_probe_fixture(&[
        ("src/a.rs", old_chain, shifted_single),
        ("src/b.rs", old_single, new_chain),
        ("src/c.rs", old_chain, new_chain),
        ("src/d.rs", old_chain, deleted),
    ])?;
    assert!(fixture.patch.contains("@@ -0,0 +1 @@"), "{}", fixture.patch);
    assert!(fixture.patch.contains("@@ -3 +4 @@"), "{}", fixture.patch);
    assert!(fixture.patch.contains("@@ -3 +3 @@"), "{}", fixture.patch);
    assert!(fixture.patch.contains("@@ -3 +2,0 @@"), "{}", fixture.patch);
    assert_eq!(
        fixture
            .files
            .iter()
            .map(|file| (
                file.path.to_str(),
                file.removed_lines
                    .iter()
                    .map(|line| (line.line, line.new_side_line))
                    .collect::<Vec<_>>(),
                file.replacement_counterparts()
                    .iter()
                    .map(|(new, old)| (*new, old.line))
                    .collect::<Vec<_>>(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some("src/a.rs"), vec![(3, 4)], vec![(4, 3)]),
            (Some("src/b.rs"), vec![(3, 3)], vec![(3, 3)]),
            (Some("src/c.rs"), vec![(3, 3)], vec![(3, 3)]),
            (Some("src/d.rs"), vec![(3, 2)], vec![]),
        ]
    );
    // Added/deleted conjuncts retain current arm heads. The unchanged-chain
    // twin has only its predicate. MatchArm bytes identify the => token;
    // the UTF-8 prefix adds six bytes, not five, to a.rs's parser identities.
    assert_eq!(
        identities(&fixture, true)?,
        vec![
            LiteralIdentity {
                file: "src/a.rs",
                line: 4,
                family: "match_arm",
                expression: "n if n < 0 =>",
                before: Some("n if m > 0 && n <= 0 =>"),
                after: Some("n if n < 0 =>"),
                start_byte: Some(72),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/a.rs",
                line: 4,
                family: "predicate",
                expression: "n < 0",
                before: Some("m > 0 && n <= 0"),
                after: Some("n < 0"),
                start_byte: Some(66),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/b.rs",
                line: 3,
                family: "match_arm",
                expression: "n if m > 0 && n < 0 =>",
                before: Some("n if n <= 0 =>"),
                after: Some("n if m > 0 && n < 0 =>"),
                start_byte: Some(75),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/b.rs",
                line: 3,
                family: "predicate",
                expression: "m > 0 && n < 0",
                before: Some("n <= 0"),
                after: Some("m > 0 && n < 0"),
                start_byte: Some(60),
                currentness: "candidate_current",
            },
            LiteralIdentity {
                file: "src/c.rs",
                line: 3,
                family: "predicate",
                expression: "m > 0 && n < 0",
                before: Some("m > 0 && n <= 0"),
                after: Some("m > 0 && n < 0"),
                start_byte: Some(60),
                currentness: "candidate_current",
            },
        ]
    );
    // d.rs deletes the same old text/line that c.rs represents. A shared
    // file/global marker pool must not consume its genuinely unpaired history.
    assert_eq!(
        identities(&fixture, false)?,
        vec![
            LiteralIdentity {
                file: "src/d.rs",
                line: 2,
                family: "match_arm",
                expression: "n if m > 0 && n <= 0 => -1,",
                before: Some("n if m > 0 && n <= 0 => -1,"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
            LiteralIdentity {
                file: "src/d.rs",
                line: 2,
                family: "predicate",
                expression: "n if m > 0 && n <= 0 => -1,",
                before: Some("n if m > 0 && n <= 0 => -1,"),
                after: None,
                start_byte: None,
                currentness: "base_deleted",
            },
        ]
    );
    Ok(())
}

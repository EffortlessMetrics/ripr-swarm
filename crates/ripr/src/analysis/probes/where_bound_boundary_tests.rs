//! Compile-time bounds must not acquire runtime field authority (#7251).

use super::diff::probes_for_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::facts::OwnedRustIndex;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::{LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{Probe, ProbeFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn probes(source: &str, changed: ChangedFile) -> Result<Vec<Probe>, String> {
    let facts = RaRustSyntaxAdapter.summarize_file(&changed.path, source)?;
    let index = RustIndex::from_owned(OwnedRustIndex {
        files: BTreeMap::from([(changed.path.clone(), facts)]),
        ..Default::default()
    });
    Ok(probes_for_file(Path::new("."), &changed, &index))
}

fn added(source: &str, line: usize) -> Result<Vec<Probe>, String> {
    let text = source
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("missing fixture line {line}"))?;
    probes(
        source,
        ChangedFile {
            path: PathBuf::from("src/lib.rs"),
            added_lines: vec![ChangedLine {
                line,
                new_side_line: line,
                text: text.into(),
            }],
            removed_lines: Vec::new(),
        },
    )
}

#[test]
fn where_bound_lines_retain_unknown_subjects() -> Result<(), String> {
    for bound in [
        "D: Deserializer<'de>,",
        "T: Deserialize<'de>,",
        "A: MapAccess<'de>,",
        "T: Iterator<Item = Vec<Option<u8>>> + Clone,",
        "T: Fn(u8) -> Vec<Option<u8>> + Send,",
        "for<'a> &'a T: IntoIterator<Item = &'a u8>,",
        "'de: 'a,",
    ] {
        for newline in ["\n", "\r\n"] {
            for owner in [
                "fn read<'de, 'a, D, T, A>()",
                "impl Reader { fn read<'de, 'a, D, T, A>()",
            ] {
                let close = if owner.starts_with("impl") { "}\n" } else { "" };
                let source =
                    format!("{owner}\nwhere\n    {bound}\n{{}}\n{close}").replace('\n', newline);
                let result = added(&source, 3)?;
                assert!(!result.is_empty(), "bound vanished: {source}");
                assert!(
                    result
                        .iter()
                        .all(|p| p.family == ProbeFamily::StaticUnknown),
                    "runtime bound: {result:?}"
                );
                assert!(
                    result
                        .iter()
                        .any(|p| p.expression == bound && p.location.line == 3)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn where_bound_shared_body_and_uppercase_fields_keep_runtime_shapes() -> Result<(), String> {
    for source in [
        "fn make<T>() where T: Clone { Packet { Upper: Marker } }\n",
        "fn make<T>()\nwhere T: Clone { Packet { Upper: construct(), ..base() } }\n",
        "fn make<T>()\nwhere\n    T: Clone,\n{\n    Packet {\n        Upper: construct(),\n        ..base()\n    }\n}\n",
    ] {
        let line = source
            .lines()
            .position(|s| s.contains("Upper:"))
            .ok_or_else(|| "missing runtime fixture".to_string())?
            + 1;
        let result = added(source, line)?;
        assert!(
            result
                .iter()
                .any(|p| p.family == ProbeFamily::FieldConstruction
                    && p.expression.contains("Upper:")),
            "lost runtime field: {result:?}"
        );
    }
    Ok(())
}

#[test]
fn bound_spelling_in_macro_or_string_has_no_declaration_authority() -> Result<(), String> {
    for (source, line) in [
        ("tokens! {\n    T: Clone,\n}\n", 2),
        ("fn f() { let text = r#\"\n    T: Clone,\n\"#; }\n", 2),
        ("fn f() { /*\n    T: Clone,\n*/ }\n", 2),
    ] {
        let result = added(source, line)?;
        assert!(
            result
                .iter()
                .any(|p| p.family == ProbeFamily::FieldConstruction),
            "invented declaration authority: {result:?}"
        );
    }
    Ok(())
}

#[test]
fn removed_field_cannot_borrow_candidate_bound_authority() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Clone,\n{}\n";
    let result = probes(
        source,
        ChangedFile {
            path: PathBuf::from("src/lib.rs"),
            added_lines: Vec::new(),
            removed_lines: vec![ChangedLine {
                line: 8,
                new_side_line: 3,
                text: "Upper: construct(),".into(),
            }],
        },
    )?;
    assert!(
        result
            .iter()
            .any(|p| p.family == ProbeFamily::FieldConstruction
                && p.before.as_deref() == Some("Upper: construct(),")
                && p.after.is_none()),
        "borrowed new-side declaration: {result:?}"
    );
    Ok(())
}

#[test]
fn edited_existing_bound_keeps_before_and_static_limitation() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Clone + Send,\n{}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,1 +3,1 @@\n-    T: Clone,\n+    T: Clone + Send,\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff)
        .into_iter()
        .next()
        .ok_or_else(|| "edited-bound diff yielded no subject".to_string())?;
    assert_eq!(changed.added_lines.len(), 1);
    assert_eq!(changed.removed_lines.len(), 1);
    let result = probes(source, changed)?;
    assert!(
        result.iter().any(|p| p.family == ProbeFamily::StaticUnknown
            && p.after.as_deref() == Some("T: Clone + Send,")),
        "lost edited bound: {result:?}"
    );
    assert!(
        result
            .iter()
            .any(|p| p.before.as_deref() == Some("T: Clone,")),
        "lost old-side evidence: {result:?}"
    );
    assert!(
        result
            .iter()
            .all(|p| p.family == ProbeFamily::StaticUnknown),
        "old bound acquired runtime authority: {result:?}"
    );
    Ok(())
}

#[test]
fn removed_only_bound_uses_old_parser_context() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Clone,\n{}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,4 @@\n fn f<T>()\n where\n-    T: Send,\n     T: Clone,\n {}\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff)
        .into_iter()
        .next()
        .ok_or_else(|| "diff fixture yielded no changed file".to_string())?;
    assert_eq!(changed.removed_lines.len(), 1);
    let result = probes(source, changed)?;
    assert_eq!(
        result.len(),
        1,
        "removed bound must stay visible: {result:?}"
    );
    assert_eq!(result[0].family, ProbeFamily::StaticUnknown);
    assert_eq!(result[0].before.as_deref(), Some("T: Send,"));
    assert!(result[0].after.is_none());
    Ok(())
}

#[test]
fn new_function_diff_keeps_bounds_unknown_and_body_executable() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Clone,\n{\n    Packet { Upper: construct() }\n}\n";
    let diff = "--- /dev/null\n+++ b/src/lib.rs\n@@ -0,0 +1,6 @@\n+fn f<T>()\n+where\n+    T: Clone,\n+{\n+    Packet { Upper: construct() }\n+}\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff)
        .into_iter()
        .next()
        .ok_or_else(|| "new-function diff yielded no subject".to_string())?;
    assert_eq!(changed.added_lines.len(), 6);
    let result = probes(source, changed)?;
    assert!(
        result
            .iter()
            .any(|p| p.family == ProbeFamily::StaticUnknown && p.expression == "T: Clone,"),
        "lost new bound: {result:?}"
    );
    assert!(
        result
            .iter()
            .any(|p| p.family == ProbeFamily::FieldConstruction && p.expression.contains("Upper:")),
        "new bound erased runtime body: {result:?}"
    );
    Ok(())
}

#[test]
fn changed_source_mismatch_cannot_admit_a_removed_bound() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Clone,\n{}\n";
    let result = probes(
        source,
        ChangedFile {
            path: PathBuf::from("src/lib.rs"),
            added_lines: vec![ChangedLine {
                line: 3,
                new_side_line: 3,
                text: "T: Unavailable,".into(),
            }],
            removed_lines: vec![ChangedLine {
                line: 3,
                new_side_line: 3,
                text: "Upper: construct(),".into(),
            }],
        },
    )?;
    assert!(
        result
            .iter()
            .any(|p| p.family == ProbeFamily::FieldConstruction
                && p.before.as_deref() == Some("Upper: construct(),")
                && p.after.is_none()),
        "source mismatch fabricated old declaration authority: {result:?}"
    );
    Ok(())
}

#[test]
fn removed_bound_after_earlier_insertion_uses_old_coordinates() -> Result<(), String> {
    let source = "// inserted\nfn f<T>()\nwhere\n    T: Clone,\n{}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n+// inserted\n fn f<T>()\n where\n     T: Clone,\n@@ -4,2 +5,1 @@\n-    T: Send,\n {}\n";
    for source in [source.to_string(), source.replace('\n', "\r\n")] {
        let changed = crate::analysis::diff::parse_unified_diff(diff)
            .into_iter()
            .next()
            .ok_or_else(|| "shifted diff yielded no subject".to_string())?;
        assert_eq!(changed.removed_lines.len(), 1);
        assert_eq!(changed.removed_lines[0].line, 4);
        let result = probes(&source, changed)?;
        assert!(
            result.iter().any(|p| p.family == ProbeFamily::StaticUnknown
                && p.before.as_deref() == Some("T: Send,")
                && p.after.is_none()),
            "shifted old-side bound lost parser ownership: {result:?}"
        );
    }
    Ok(())
}

#[test]
fn identical_added_bound_cannot_hide_removed_runtime_field() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Clone,\n{}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,4 @@\n-fn f<T>() {\n-    Packet {\n-        T: Clone,\n-    }\n-}\n+fn f<T>()\n+where\n+    T: Clone,\n+{}\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff)
        .into_iter()
        .next()
        .ok_or_else(|| "collision diff yielded no subject".to_string())?;
    assert_eq!(changed.removed_lines.len(), 5);
    let result = probes(source, changed)?;
    assert!(
        result
            .iter()
            .any(|p| p.family == ProbeFamily::FieldConstruction
                && p.before.as_deref() == Some("T: Clone,")
                && p.after.is_none()),
        "new-side bound hid the same-spelled removed runtime field: {result:?}"
    );
    assert!(
        result
            .iter()
            .any(|p| p.family == ProbeFamily::StaticUnknown
                && p.after.as_deref() == Some("T: Clone,")),
        "candidate bound was not independently retained: {result:?}"
    );
    Ok(())
}

#[test]
fn multiline_generic_parameters_and_where_predicates_stay_unknown() -> Result<(), String> {
    for (source, lines) in [
        ("fn f<\n    T: Clone,\n>() {}\n", vec![2]),
        (
            "fn f<T>()\nwhere\n    T: Iterator<\n        Item = Vec<Option<u8>>\n    > + Clone,\n{}\n",
            vec![3, 4, 5],
        ),
        ("fn f<T>()\nwhere T: Clone,\n    T: Send,\n{}\n", vec![2, 3]),
    ] {
        for line in lines {
            let result = added(source, line)?;
            assert!(!result.is_empty(), "lost multiline bound: {source}:{line}");
            assert!(
                result
                    .iter()
                    .all(|p| p.family == ProbeFamily::StaticUnknown),
                "runtime multiline bound: {result:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn malformed_bound_is_not_silently_dropped_or_credited() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: ,\n{\n";
    let path = PathBuf::from("src/lib.rs");
    if RaRustSyntaxAdapter.summarize_file(&path, source).is_ok() {
        return Err("malformed fixture was unexpectedly accepted by the parser".into());
    }
    let facts = LexicalRustSyntaxAdapter.summarize_file(&path, source)?;
    assert!(facts.used_lexical_fallback);
    let index = RustIndex::from_owned(OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let changed = ChangedFile {
        path,
        added_lines: vec![ChangedLine {
            line: 3,
            new_side_line: 3,
            text: "T: ,".into(),
        }],
        removed_lines: Vec::new(),
    };
    let result = probes_for_file(Path::new("."), &changed, &index);
    assert!(!result.is_empty(), "unsupported syntax vanished");
    assert!(
        result.iter().all(|p| p.after.is_some()),
        "unsupported syntax lost its changed-source evidence"
    );
    Ok(())
}

#[test]
fn collapsed_bound_replacement_retains_every_removed_predicate() -> Result<(), String> {
    let source = "fn f<T>()\nwhere\n    T: Sync,\n{}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,4 @@\n fn f<T>()\n where\n-    T: Clone,\n-    T: Send,\n+    T: Sync,\n {}\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff)
        .into_iter()
        .next()
        .ok_or_else(|| "collapsed-bound diff yielded no subject".to_string())?;
    let result = probes(source, changed)?;
    for old in ["T: Clone,", "T: Send,"] {
        assert!(
            result.iter().any(|p| p.family == ProbeFamily::StaticUnknown
                && p.before.as_deref() == Some(old)),
            "removed predicate vanished: {old}: {result:?}"
        );
    }
    Ok(())
}

#[test]
fn expression_and_macro_bounds_keep_fallback_evidence() -> Result<(), String> {
    for bound in ["T: Trait<{ construct() }>,", "T: Trait<types!()>,"] {
        let source = format!("fn f<T>()\nwhere\n    {bound}\n{{}}\n");
        let result = added(&source, 3)?;
        assert!(!result.is_empty(), "unsupported bound vanished: {bound}");
        assert!(
            result.iter().any(|p| p.after.is_some()),
            "unsupported bound lost source evidence: {result:?}"
        );
    }
    Ok(())
}

#[test]
fn removed_bound_cannot_borrow_candidate_test_role() -> Result<(), String> {
    let source = "#[test] fn retained() {}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,1 @@\n-fn f<T>()\n-where\n-    T: Send,\n-{}\n #[test] fn retained() {}\n";
    let changed = crate::analysis::diff::parse_unified_diff(diff)
        .into_iter()
        .next()
        .ok_or_else(|| "no changed file".to_string())?;
    assert_eq!(changed.removed_lines.len(), 4);
    let result = probes(source, changed)?;
    assert!(
        result.iter().any(|p| p.family == ProbeFamily::StaticUnknown
            && p.before.as_deref() == Some("T: Send,")
            && p.after.is_none()),
        "base-side bound vanished through candidate test role: {result:?}"
    );
    Ok(())
}

#[test]
fn removed_test_bound_cannot_borrow_candidate_production_role() -> Result<(), String> {
    for deleted in [
        "#[cfg(test)]\nmod checks {\nfn helper<T>()\nwhere\n    T: Send,\n{}\n}\n",
        "#[cfg(test)]\nmod checks {\nstruct Packet<T>\nwhere\n    T: Send,\n{ value: T }\n}\n",
    ] {
        let source = "fn retained() {}\n";
        let old_source = format!("{deleted}{source}");
        RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &old_source)?;
        let deletions = deleted
            .lines()
            .map(|line| format!("-{line}\n"))
            .collect::<String>();
        let diff = format!(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{} +1,1 @@\n{deletions} {source}",
            deleted.lines().count() + 1
        );
        let changed = crate::analysis::diff::parse_unified_diff(&diff)
            .into_iter()
            .next()
            .ok_or_else(|| "no removed-test subject".to_string())?;
        assert!(
            changed
                .removed_lines
                .iter()
                .any(|line| line.text.trim() == "T: Send,")
        );
        let result = probes(source, changed)?;
        assert!(
            !result
                .iter()
                .any(|p| p.before.as_deref() == Some("T: Send,")),
            "old evidence-only bound became a production subject: {result:?}"
        );
    }
    Ok(())
}

#[test]
fn removed_bound_preserves_composed_test_file_exclusion() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-removed-bound-context-{stamp}"));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let result = (|| {
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='bound-context'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        for owner in ["fn helper<T>()", "struct Packet<T>"] {
            let source = format!("{owner}\nwhere\n    T: Clone,\n{{}}\n");
            std::fs::write(root.join("src/checks.rs"), &source)
                .map_err(|error| error.to_string())?;
            let diff = format!(
                "--- a/src/checks.rs\n+++ b/src/checks.rs\n@@ -1,5 +1,4 @@\n {owner}\n where\n-    T: Send,\n     T: Clone,\n {{}}\n"
            );
            let changed = crate::analysis::diff::parse_unified_diff(&diff)
                .into_iter()
                .next()
                .ok_or_else(|| "missing removed bound".to_string())?;
            assert_eq!(changed.removed_lines[0].text.trim(), "T: Send,");
            for test_only in [false, true] {
                let parent = if test_only {
                    "#[cfg(test)]\nmod checks;\n"
                } else {
                    "mod checks;\n"
                };
                std::fs::write(root.join("src/lib.rs"), parent)
                    .map_err(|error| error.to_string())?;
                let index = crate::analysis::facts::build_index(
                    &root,
                    &[PathBuf::from("src/lib.rs"), PathBuf::from("src/checks.rs")],
                )?;
                let facts = crate::analysis::rust_index::find_file_facts(&index, &changed.path)
                    .ok_or_else(|| "missing indexed child".to_string())?;
                assert_eq!(
                    facts
                        .role_provenance
                        .edges
                        .iter()
                        .any(|edge| edge.requires_test),
                    test_only
                );
                let result = probes_for_file(&root, &changed, &index);
                let retained = result.iter().any(|probe| {
                    probe.family == ProbeFamily::StaticUnknown
                        && probe.before.as_deref() == Some("T: Send,")
                        && probe.after.is_none()
                });
                assert_eq!(
                    retained, !test_only,
                    "composed file exclusion for {owner}, test_only={test_only}: {result:?}"
                );
            }
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    result
}

#[test]
fn removed_bounds_use_normalized_test_attributes() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-old-bound-attributes-{stamp}"));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let result = (|| {
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='bound-attributes'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        for (attribute, is_test) in [
            ("#[quickcheck]", true),
            ("#[test_case(1)]", true),
            ("#[rstest::rstest]", true),
            ("#[tokio::test_helper]", false),
        ] {
            let source = format!("{attribute}\nfn subject<T>()\nwhere\n    T: Clone,\n{{}}\n");
            std::fs::write(root.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
            let index = crate::analysis::facts::build_index(&root, &[PathBuf::from("src/lib.rs")])?;
            let owner = crate::analysis::rust_index::find_owner_function(
                &index,
                Path::new("src/lib.rs"),
                4,
            )
            .ok_or_else(|| "missing attribute owner".to_string())?;
            assert_eq!(owner.source_role.is_evidence_role(), is_test);
            let diff = format!(
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,5 @@\n {attribute}\n fn subject<T>()\n where\n-    T: Send,\n     T: Clone,\n {{}}\n"
            );
            let changed = crate::analysis::diff::parse_unified_diff(&diff)
                .into_iter()
                .next()
                .ok_or_else(|| "missing attribute bound".to_string())?;
            let probes = probes_for_file(&root, &changed, &index);
            assert_eq!(
                probes
                    .iter()
                    .any(|probe| probe.before.as_deref() == Some("T: Send,")),
                !is_test,
                "normalized old role for {attribute}: {probes:?}"
            );
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    result
}

#[test]
fn removed_bounds_use_registered_old_attributes() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-old-bound-registry-{stamp}"));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    let result = (|| {
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='bound-registry'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        let registrations = [crate::config::TestHarnessRegistration {
            registration_id: "bounds".into(),
            target: PathBuf::from("src/lib.rs"),
            kind: crate::config::TestHarnessKind::RegisteredAttribute,
            adapter: crate::config::TestHarnessAdapter::ExactAttributeV1,
            marker: "custom::check".into(),
        }];
        for (prefix, candidate, excluded) in [
            ("#[custom::check]\n", "fn retained() {}\n", true),
            ("use custom::check;\n#[check]\n", "fn retained() {}\n", true),
            (
                "use custom::check as old_check;\n#[old_check]\n",
                "fn retained() {}\n",
                false,
            ),
            ("#[custom::check_helper]\n", "fn retained() {}\n", false),
            ("", "#[custom::check] fn retained() {}\n", false),
        ] {
            let deleted = format!("{prefix}fn subject<T>()\nwhere\n    T: Send,\n{{}}\n");
            std::fs::write(root.join("src/lib.rs"), format!("{deleted}{candidate}"))
                .map_err(|error| error.to_string())?;
            let old_index = crate::analysis::facts::build_index_with_test_harnesses(
                &root,
                &[PathBuf::from("src/lib.rs")],
                &registrations,
            )?;
            let old_owner = crate::analysis::rust_index::find_owner_function(
                &old_index,
                Path::new("src/lib.rs"),
                prefix.lines().count() + 1,
            )
            .ok_or_else(|| "missing old registry owner".to_string())?;
            assert_eq!(
                old_owner.source_role.is_evidence_role(),
                excluded,
                "unsupported registry fixture premise: {prefix:?}"
            );
            std::fs::write(root.join("src/lib.rs"), candidate)
                .map_err(|error| error.to_string())?;
            let index = crate::analysis::facts::build_index_with_test_harnesses(
                &root,
                &[PathBuf::from("src/lib.rs")],
                &registrations,
            )?;
            let deletions = deleted
                .lines()
                .map(|line| format!("-{line}\n"))
                .collect::<String>();
            let diff = format!(
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{} +1,1 @@\n{deletions} {candidate}",
                deleted.lines().count() + 1
            );
            let changed = crate::analysis::diff::parse_unified_diff(&diff)
                .into_iter()
                .next()
                .ok_or_else(|| "missing registered bound".to_string())?;
            assert!(
                changed
                    .removed_lines
                    .iter()
                    .any(|line| line.text.trim() == "T: Send,")
            );
            let probes = super::diff::probes_for_file_with_relations(
                &root,
                &changed,
                &index,
                &registrations,
            );
            assert_eq!(
                probes
                    .iter()
                    .any(|seeded| seeded.probe.family == ProbeFamily::StaticUnknown
                        && seeded.probe.before.as_deref() == Some("T: Send,")),
                !excluded,
                "registered old role for {prefix:?}, candidate={candidate:?}: {probes:?}"
            );
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    result
}

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

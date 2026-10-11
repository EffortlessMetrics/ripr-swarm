//! Record-field declarations and initializers require distinct source identities.
//! Regression for EffortlessMetrics/ripr#1453 and ub-review#1306.

use super::classify::parser_probe_shapes_for_changed_line;
use super::diff::probes_for_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::{ProbeShapeKind, RustIndex};
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{Probe, ProbeFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SOURCE: &str = "struct Marker;\nstruct Packet {\n    value: Marker,\n}\nfn packet() -> Packet {\n    Packet {\n        value: Marker,\n    }\n}\n";

/// Build the real RA summary and diff-produced probes for one exact changed line.
fn probes_at(source: &str, line: usize) -> Result<Vec<Probe>, String> {
    let path = PathBuf::from("src/lib.rs");
    let text = source
        .lines()
        .nth(line.saturating_sub(1))
        .ok_or_else(|| format!("fixture has no line {line}"))?
        .to_string();
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let changed = ChangedFile {
        path,
        added_lines: vec![ChangedLine {
            line,
            new_side_line: line,
            text,
        }],
        removed_lines: Vec::new(),
    };
    Ok(probes_for_file(Path::new("."), &changed, &index))
}

#[test]
fn record_field_declaration_retains_exact_unknown_subject() -> Result<(), String> {
    let probes = probes_at(SOURCE, 3)?;
    assert_eq!(
        probes.len(),
        1,
        "declaration must remain visible: {probes:?}"
    );
    assert_eq!(
        probes[0].family,
        ProbeFamily::StaticUnknown,
        "a field declaration is not an executable initializer"
    );
    assert_eq!(probes[0].location.line, 3);
    assert_eq!(probes[0].expression, "value: Marker,");
    Ok(())
}

#[test]
fn identical_record_initializer_retains_executable_subject() -> Result<(), String> {
    let probes = probes_at(SOURCE, 7)?;
    assert!(
        probes.iter().any(|probe| {
            probe.family == ProbeFamily::FieldConstruction
                && probe.location.line == 7
                && probe.expression.contains("value: Marker")
        }),
        "record declaration handling erased an actual initializer: {probes:?}"
    );
    Ok(())
}

#[test]
fn commented_record_fields_and_initializers_keep_distinct_families() -> Result<(), String> {
    for suffix in [" // field", " /* outer /* nested */ tail */"] {
        for newline in ["\n", "\r\n"] {
            let text = format!("value: Marker,{suffix}");
            let source = SOURCE
                .replace("value: Marker,", &text)
                .replace('\n', newline);
            let declarations = probes_at(&source, 3)?;
            assert_eq!(declarations.len(), 1);
            assert_eq!(declarations[0].family, ProbeFamily::StaticUnknown);
            assert_eq!(declarations[0].location.line, 3);
            assert_eq!(declarations[0].expression, text);
            let initializers = probes_at(&source, 7)?;
            assert!(
                initializers.iter().any(|probe| {
                    probe.family == ProbeFamily::FieldConstruction && probe.location.line == 7
                }),
                "comment handling erased the real initializer: {initializers:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn nested_record_declaration_retains_its_unsafe_boundary() -> Result<(), String> {
    let source = "unsafe fn packet() {\n    struct Packet {\n        value: u8,\n    }\n}\n";
    let path = PathBuf::from("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    let boundary = facts
        .probe_shapes
        .iter()
        .find(|shape| shape.kind == ProbeShapeKind::UnsafeBoundary)
        .cloned()
        .ok_or_else(|| "fixture has no unsafe boundary".to_string())?;
    let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let shapes = parser_probe_shapes_for_changed_line(&index, &path, 3, "value: u8,");
    assert_eq!(shapes.len(), 2);
    let declaration = shapes
        .iter()
        .find(|shape| !shape.unsafe_boundary)
        .ok_or_else(|| "unsafe boundary hid the declaration".to_string())?;
    assert_eq!(declaration.family, ProbeFamily::StaticUnknown);
    assert_eq!(declaration.start_line, 3);
    assert_eq!(Some(declaration.start_byte), source.find("value: u8,"));
    assert_eq!(declaration.text, "value: u8,");
    let retained = shapes
        .iter()
        .find(|shape| shape.unsafe_boundary)
        .ok_or_else(|| "record field handling erased the unsafe boundary".to_string())?;
    assert_eq!(retained.start_byte, boundary.start_byte);
    assert_eq!(retained.text, boundary.text);
    let probes = probes_at(source, 3)?;
    assert!(probes.iter().any(|probe| probe.expression == "value: u8,"));
    assert!(probes.iter().any(|probe| probe.expression == boundary.text));
    Ok(())
}

#[test]
fn shared_record_definition_and_body_keep_the_real_initializer() -> Result<(), String> {
    let source = "struct Marker;\nstruct Packet { value: Marker } fn packet() -> Packet { Packet { value: Marker } }\n";
    let probes = probes_at(source, 2)?;
    assert!(
        probes.iter().any(|probe| {
            probe.family == ProbeFamily::FieldConstruction
                && probe.expression.contains("value: Marker")
        }),
        "shared declaration line erased executable field evidence: {probes:?}"
    );
    Ok(())
}

/// Real RA summary and diff probes for one replaced line, so the probe sees
/// the removed text the diff pairs with it.
fn probes_for_replaced_line(
    source: &str,
    line: usize,
    removed: &str,
) -> Result<Vec<Probe>, String> {
    probes_for_replaced_block(source, line, &[removed])
}

/// Real RA summary and diff probes for a block replacing `removed.len()`
/// lines starting at `first_line`. Like the diff parser, every removed line
/// carries the new-side coordinate where the added run starts.
fn probes_for_replaced_block(
    source: &str,
    first_line: usize,
    removed: &[&str],
) -> Result<Vec<Probe>, String> {
    let path = PathBuf::from("src/lib.rs");
    let added_lines = (first_line..first_line + removed.len())
        .map(|line| {
            source
                .lines()
                .nth(line.saturating_sub(1))
                .map(|text| ChangedLine {
                    line,
                    new_side_line: line,
                    text: text.to_string(),
                })
                .ok_or_else(|| format!("fixture has no line {line}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let changed = ChangedFile {
        path,
        added_lines,
        removed_lines: removed
            .iter()
            .enumerate()
            .map(|(offset, text)| ChangedLine {
                line: first_line + offset,
                new_side_line: first_line,
                text: (*text).to_string(),
            })
            .collect(),
    };
    Ok(probes_for_file(Path::new("."), &changed, &index))
}

fn field_construction_expressions(probes: &[Probe]) -> Vec<&str> {
    probes
        .iter()
        .filter(|probe| probe.family == ProbeFamily::FieldConstruction)
        .map(|probe| probe.expression.as_str())
        .collect()
}

const ONE_LINE_ID: &str = "pub struct Id {\n    counter: u32,\n    version: u8,\n}\npub fn new_v1() -> Id {\n    Id { counter: 0x00ab_cdef, version: 0x1 }\n}\n";

#[test]
fn one_line_struct_literal_probes_the_edited_field_not_its_neighbour() -> Result<(), String> {
    // #6731: `counter` sorts first, but only `version` changed.
    let probes = probes_for_replaced_line(
        ONE_LINE_ID,
        6,
        "    Id { counter: 0x00ab_cdef, version: 1 }",
    )?;
    assert_eq!(
        field_construction_expressions(&probes),
        vec!["version: 0x1"],
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn a_field_text_inside_a_longer_removed_value_is_not_unchanged() -> Result<(), String> {
    // `version: 0x1` occurs inside the removed `version: 0x10`, but that
    // field still changed; only a whole-token match counts as unchanged.
    let probes = probes_for_replaced_line(
        ONE_LINE_ID,
        6,
        "    Id { counter: 0x00ab_cdef, version: 0x10 }",
    )?;
    assert_eq!(
        field_construction_expressions(&probes),
        vec!["version: 0x1"],
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn a_removed_comment_repeating_the_new_field_does_not_mark_it_unchanged() -> Result<(), String> {
    // The old line's comment already reads `version: 0x2`; only code counts
    // as the unchanged field, so the edited `version` stays the subject.
    let source = "pub struct Id {\n    counter: u32,\n    version: u8,\n}\npub fn new_v2() -> Id {\n    Id { counter: 0x00ab_cdef, version: 0x2 } // version: 0x2\n}\n";
    let probes = probes_for_replaced_line(
        source,
        6,
        "    Id { counter: 0x00ab_cdef, version: 0x1 } // version: 0x2",
    )?;
    assert_eq!(
        field_construction_expressions(&probes),
        vec!["version: 0x2"],
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn adjacent_replaced_literals_pair_with_their_own_removed_lines() -> Result<(), String> {
    // Both removed lines share the `Id` token with both added lines. The
    // second added line must compare against the second removed line, where
    // `b: 2` is new, not the first, where `b: 2` already appeared. Each
    // `before` is cut to the field span (#5312); the pairing still reads off
    // it because `b: 3` only exists on the second removed line.
    let source = "pub struct Id {\n    a: u8,\n    b: u8,\n}\npub fn pair() -> (Id, Id) {\n    (\n        Id { a: 0, b: 4 },\n        Id { a: 1, b: 2 },\n    )\n}\n";
    let probes = probes_for_replaced_block(
        source,
        7,
        &["        Id { a: 0, b: 2 },", "        Id { a: 1, b: 3 },"],
    )?;
    // Removed-side probes (no `after`) are out of scope here.
    let added_side = probes
        .iter()
        .filter(|probe| probe.after.is_some())
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        field_construction_expressions(&added_side),
        vec!["b: 4", "b: 2"],
        "{probes:?}"
    );
    let befores = added_side
        .iter()
        .map(|probe| probe.before.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(befores, vec![Some("b: 2"), Some("b: 3")], "{probes:?}");
    Ok(())
}

#[test]
fn a_field_matching_another_literal_on_the_same_line_is_still_edited() -> Result<(), String> {
    // Only the first literal's `version` changed, to the text the second
    // literal already had. The old line holds one `version: 2`, the new line
    // two, so the first literal's `version: 2` is new.
    let source = "pub struct Id {\n    counter: u8,\n    version: u8,\n}\npub fn pair() -> (Id, Id) {\n    (Id { counter: 0, version: 2 }, Id { counter: 1, version: 2 })\n}\n";
    let probes = probes_for_replaced_line(
        source,
        6,
        "    (Id { counter: 0, version: 1 }, Id { counter: 1, version: 2 })",
    )?;
    assert_eq!(
        field_construction_expressions(&probes),
        vec!["version: 2"],
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn a_field_that_was_the_head_of_a_longer_value_is_still_edited() -> Result<(), String> {
    // The old `flag: foo && bar` begins with the new `flag: foo`, but the
    // field's value changed; the untouched `enabled: true` is not the subject.
    let source = "pub struct Packet {\n    flag: bool,\n    enabled: bool,\n}\npub fn packet(foo: bool) -> Packet {\n    Packet { flag: foo, enabled: true }\n}\n";
    let probes =
        probes_for_replaced_line(source, 6, "    Packet { flag: foo && bar, enabled: true }")?;
    assert_eq!(
        field_construction_expressions(&probes),
        vec!["flag: foo"],
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn an_unchanged_call_beside_an_operator_still_reads_unchanged() -> Result<(), String> {
    // The whole-unit rule is for record fields only. `f(a)` follows `=` and
    // precedes `+`, yet it is the same call on both lines; `g(b)` is the edit.
    let source = "fn f(v: u8) -> u8 { v }\nfn g(v: u8) -> u8 { v }\npub fn sum(a: u8, b: u8) -> u8 {\n    let x = f(a) + g(b);\n    x\n}\n";
    let path = PathBuf::from("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let shapes = super::classify::parser_probe_shapes_for_changed_line_against(
        &index,
        &path,
        4,
        "let x = f(a) + g(b);",
        Some("let x = f(a) + g(c);"),
    );
    let calls = shapes
        .iter()
        .filter(|shape| shape.family == ProbeFamily::CallDeletion)
        .map(|shape| shape.text)
        .collect::<Vec<_>>();
    assert_eq!(calls, vec!["g(b)"], "{calls:?}");
    Ok(())
}

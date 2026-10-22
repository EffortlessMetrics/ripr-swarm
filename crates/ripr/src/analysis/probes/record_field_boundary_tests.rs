//! Record-field declarations and initializers require distinct source identities.
//! Regression for EffortlessMetrics/ripr#1453 and ub-review#1306.

use super::classify::parser_probe_shapes_for_changed_line;
use super::diff::probes_for_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::{ProbeShapeKind, RustIndex};
use crate::analysis::syntax::{LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter};
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

/// #8450: a literally empty `fn` body elsewhere in the file must not widen
/// a later field-construction probe to the whole changed line. The parser
/// shape is the field initializer; the whole-line array is the lexical
/// fallback that hides the element pin.
fn rows_source_with_prefix(prefix: &str) -> String {
    format!(
        "pub struct Row {{
    pub total: u32,
}}
{prefix}
pub fn rows(a: u32, b: u32) -> [Row; 2] {{
    [Row {{ total: 0 }}, Row {{ total: a + b + 0 }}]
}}
"
    )
}

fn field_line_of(source: &str) -> Result<usize, String> {
    source
        .lines()
        .position(|line| line.contains("a + b + 0"))
        .map(|index| index + 1)
        .ok_or_else(|| format!("fixture lost the changed field line: {source}"))
}

fn field_construction_debug(source: &str) -> Result<(Vec<String>, String), String> {
    let path = PathBuf::from("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    if facts.used_lexical_fallback {
        return Err(format!(
            "parser must keep this source; lexical fallback would force the whole line: {source}"
        ));
    }
    let rows = facts
        .functions
        .iter()
        .find(|function| function.name == "rows")
        .ok_or_else(|| format!("rows was not summarized: {source}"))?;
    if !rows.body.as_str().contains("total: a + b + 0") {
        return Err(format!(
            "rows body lost the field initializer: {:?}",
            rows.body.as_str()
        ));
    }
    let line = field_line_of(source)?;
    let shapes: Vec<String> = facts
        .probe_shapes
        .iter()
        .filter(|shape| shape.kind == ProbeShapeKind::FieldConstruction)
        .map(|shape| {
            format!(
                "{}:{}..{} {:?}",
                shape.start_line, shape.start_byte, shape.end_byte, shape.text
            )
        })
        .collect();
    let functions: Vec<String> = facts
        .functions
        .iter()
        .map(|function| {
            format!(
                "{} {}..{}",
                function.name, function.start_line, function.end_line
            )
        })
        .collect();
    let probes = probes_at(source, line)?;
    let expressions = field_construction_expressions(&probes)
        .into_iter()
        .map(str::to_string)
        .collect();
    let debug = format!("functions={functions:?} shapes={shapes:?} probes={probes:?}");
    Ok((expressions, debug))
}

fn assert_field_initializer(label: &str, source: &str) -> Result<(), String> {
    let (expressions, debug) = field_construction_debug(source)?;
    if !expressions
        .iter()
        .any(|expression| expression == "total: a + b + 0")
    {
        return Err(format!(
            "{label} must keep the field initializer, got {expressions:?} ({debug})"
        ));
    }
    if expressions
        .iter()
        .any(|expression| expression.contains("[Row"))
    {
        return Err(format!(
            "{label} must not fall back to the whole array line, got {expressions:?} ({debug})"
        ));
    }
    Ok(())
}

#[test]
fn an_empty_fn_body_elsewhere_still_probes_the_field_initializer() -> Result<(), String> {
    // The reported defect used `fn drop(&mut self) {}` before `rows`. These
    // siblings pin that any empty `fn` body in the file keeps the initializer,
    // including the one-line impl form and a compact `{}` with no spaces.
    for (label, prefix) in [
        (
            "multiline empty Drop",
            "impl Drop for Row {\n    fn drop(&mut self) {}\n}\n",
        ),
        (
            "one-line empty Drop",
            "impl Drop for Row { fn drop(&mut self) {} }\n",
        ),
        (
            "compact empty Drop",
            "impl Drop for Row { fn drop(&mut self){} }\n",
        ),
        (
            "comment-only Drop",
            "impl Drop for Row {\n    fn drop(&mut self) { /* no-op */ }\n}\n",
        ),
        (
            "statement Drop",
            "impl Drop for Row {\n    fn drop(&mut self) {\n        let _ = ();\n    }\n}\n",
        ),
        ("free empty fn", "fn helper() {}\n"),
        ("compact free empty fn", "fn helper(){}\n"),
        ("spaced empty fn", "fn helper() { }\n"),
        (
            "empty trait default",
            "trait Hook {\n    fn hook(&mut self) {}\n}\n",
        ),
    ] {
        assert_field_initializer(label, &rows_source_with_prefix(prefix))?;
    }
    let crlf = rows_source_with_prefix("fn helper() {}\n").replace('\n', "\r\n");
    assert_field_initializer("CRLF empty fn", &crlf)?;
    let after = format!(
        "{}fn later() {{}}\n",
        rows_source_with_prefix("fn helper() {}\n")
    );
    assert_field_initializer("empty fn after rows", &after)?;
    Ok(())
}

/// Without parser field shapes the same line is the whole array. This is
/// the degraded reading #8450 described; an empty `fn` body must not force
/// that fallback.
#[test]
fn lexical_fallback_names_the_whole_array_line() -> Result<(), String> {
    let source = rows_source_with_prefix("fn helper() {}\n");
    let path = PathBuf::from("src/lib.rs");
    let facts = LexicalRustSyntaxAdapter.summarize_file(&path, &source)?;
    if !facts.used_lexical_fallback || !facts.probe_shapes.is_empty() {
        return Err(format!(
            "lexical adapter must disclose empty parser shapes, got fallback={} shapes={}",
            facts.used_lexical_fallback,
            facts.probe_shapes.len()
        ));
    }
    let line = field_line_of(&source)?;
    let text = source
        .lines()
        .nth(line.saturating_sub(1))
        .ok_or("fixture has no field line")?
        .to_string();
    let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let probes = probes_for_file(
        Path::new("."),
        &ChangedFile {
            path,
            added_lines: vec![ChangedLine {
                line,
                new_side_line: line,
                text,
            }],
            removed_lines: Vec::new(),
        },
        &index,
    );
    let expressions = field_construction_expressions(&probes);
    if !expressions
        .iter()
        .any(|expression| expression.contains("[Row"))
    {
        return Err(format!(
            "lexical fallback must name the whole array line so the parser pin is not vacuous: {expressions:?}"
        ));
    }
    Ok(())
}

const ENUM_SOURCE: &str = "pub enum DecodeError {\n    InvalidByte(usize, u8),\n    InvalidLastSymbol {\n        offset: usize,\n        byte: u8,\n    },\n    Short { offset: usize },\n}\npub fn bad(offset: usize) -> DecodeError {\n    DecodeError::InvalidLastSymbol {\n        offset: offset,\n        byte: 0,\n    }\n}\n";

/// #8999: an enum variant's named-field declaration is type syntax; it
/// stays one visible `static_unknown` probe and never `field_construction`.
#[test]
fn enum_variant_field_declarations_are_not_field_construction() -> Result<(), String> {
    for (line, expression) in [
        (4, "offset: usize,"),
        (5, "byte: u8,"),
        (7, "Short { offset: usize },"),
    ] {
        let probes = probes_at(ENUM_SOURCE, line)?;
        assert!(
            field_construction_expressions(&probes).is_empty(),
            "line {line} declaration seeded a construction: {probes:?}"
        );
        assert_eq!(probes.len(), 1, "line {line}: {probes:?}");
        assert_eq!(probes[0].family, ProbeFamily::StaticUnknown);
        assert_eq!(probes[0].expression, expression);
    }
    Ok(())
}

/// #8999 negative control: the same field names constructing the variant in
/// a function body still seed `field_construction`.
#[test]
fn enum_variant_construction_still_seeds_field_construction() -> Result<(), String> {
    let probes = probes_at(ENUM_SOURCE, 11)?;
    assert!(
        probes.iter().any(|probe| {
            probe.family == ProbeFamily::FieldConstruction
                && probe.location.line == 11
                && probe.expression.contains("offset: offset")
        }),
        "variant construction lost its field probe: {probes:?}"
    );
    Ok(())
}

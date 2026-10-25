//! #9183: a deleted struct or enum-variant field declaration is old-source type
//! syntax, so its removed-side probe is `static_unknown`, never
//! `field_construction`. Admission is parser-owned (reconstructed old source,
//! exact line, one declaration owner); a body constructor, a discriminant, a
//! macro-typed field and a shared line keep the lexical probe, and an edited
//! field keeps its before/after pairing.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// What the new side does with one old line.
enum Op {
    Keep,
    Remove,
    Replace(&'static str),
    AddAfter(&'static str),
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// Build the new source and a whole-file unified diff from the old source
    /// and one operation per old line.
    fn create(old: &str, ops: &[Op]) -> Result<Self, String> {
        let old_lines = old.lines().collect::<Vec<_>>();
        if old_lines.len() != ops.len() {
            return Err("one operation per old line".to_string());
        }
        let mut new_source = String::new();
        let mut body = String::new();
        let mut new_count = 0;
        for (line, op) in old_lines.iter().zip(ops) {
            match op {
                Op::Keep => {
                    new_source.push_str(&format!("{line}\n"));
                    body.push_str(&format!(" {line}\n"));
                    new_count += 1;
                }
                Op::Remove => body.push_str(&format!("-{line}\n")),
                Op::Replace(text) => {
                    new_source.push_str(&format!("{text}\n"));
                    body.push_str(&format!("-{line}\n+{text}\n"));
                    new_count += 1;
                }
                Op::AddAfter(text) => {
                    new_source.push_str(&format!("{line}\n{text}\n"));
                    body.push_str(&format!(" {line}\n+{text}\n"));
                    new_count += 2;
                }
            }
        }
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{} +1,{new_count} @@\n{body}",
            old_lines.len()
        );
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-removed-field-decl-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"removed-field-decl\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), new_source)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn removed_ops(old: &str, removed_line: usize) -> Vec<Op> {
    (1..=old.lines().count())
        .map(|line| {
            if line == removed_line {
                Op::Remove
            } else {
                Op::Keep
            }
        })
        .collect()
}

/// The findings whose probe names the removed text on its old side.
fn findings_for(old: &str, ops: &[Op], removed: &str) -> Result<Vec<Finding>, String> {
    Ok(TempRepo::create(old, ops)?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.before.as_deref().map(str::trim) == Some(removed))
        .collect())
}

/// Delete line `line` of `old` (whose trimmed text is `text`) with no
/// replacement and require exactly one probe, of `family`, on the old side.
fn removed_only(
    old: &str,
    line: usize,
    text: &str,
    family: ProbeFamily,
) -> Result<Finding, String> {
    let lines = old.lines().collect::<Vec<_>>();
    if lines.get(line - 1).map(|l| l.trim()) != Some(text) {
        return Err(format!("premise: line {line} is not `{text}`"));
    }
    let mut found = findings_for(old, &removed_ops(old, line), text)?;
    if found.len() != 1 {
        return Err(format!("`{text}`: expected one probe, got {found:?}"));
    }
    let finding = found.remove(0);
    if finding.probe.family != family {
        return Err(format!(
            "`{text}`: expected {family:?}, got {:?}",
            finding.probe.family
        ));
    }
    if finding.probe.after.is_some() {
        return Err(format!("`{text}`: removed-only probe has an after side"));
    }
    Ok(finding)
}

fn require_declaration_unknown(finding: &Finding) -> Result<(), String> {
    if finding.class != ExposureClass::StaticUnknown {
        return Err(format!(
            "`{}` classified {:?}, expected static_unknown",
            finding.probe.expression, finding.class
        ));
    }
    Ok(())
}

const STRUCT_OLD: &str = "pub struct Packet {\n    pub offset: usize,\n    pub len: usize,\n}\npub fn len(packet: &Packet) -> usize {\n    packet.len\n}\n";
const ENUM_OLD: &str = "pub enum DecodeError {\n    InvalidByte(usize, u8),\n    InvalidLastSymbol {\n        offset: usize,\n        byte: u8,\n    },\n    Short { offset: usize },\n}\npub fn code(error: &DecodeError) -> u8 {\n    match error {\n        DecodeError::InvalidByte(..) => 1,\n        _ => 2,\n    }\n}\n";

#[test]
fn removed_only_struct_field_is_static_unknown() -> Result<(), String> {
    let finding = removed_only(
        STRUCT_OLD,
        2,
        "pub offset: usize,",
        ProbeFamily::StaticUnknown,
    )?;
    require_declaration_unknown(&finding)
}

#[test]
fn removed_only_enum_variant_fields_are_static_unknown() -> Result<(), String> {
    for (line, text) in [
        (4, "offset: usize,"),
        (5, "byte: u8,"),
        (7, "Short { offset: usize },"),
    ] {
        let finding = removed_only(ENUM_OLD, line, text, ProbeFamily::StaticUnknown)?;
        require_declaration_unknown(&finding)?;
    }
    Ok(())
}

#[test]
fn removed_body_constructor_keeps_field_construction() -> Result<(), String> {
    let old = "pub struct Packet {\n    pub offset: usize,\n}\npub fn make() -> Packet {\n    let first = Packet { offset: 1 };\n    first\n}\n";
    removed_only(
        old,
        5,
        "let first = Packet { offset: 1 };",
        ProbeFamily::FieldConstruction,
    )?;
    Ok(())
}

#[test]
fn removed_discriminant_variant_keeps_field_construction() -> Result<(), String> {
    let old = "#[repr(u8)]\npub enum Mode {\n    Bad { offset: usize } = 3,\n    Good = 4,\n}\n";
    removed_only(
        old,
        3,
        "Bad { offset: usize } = 3,",
        ProbeFamily::FieldConstruction,
    )?;
    Ok(())
}

#[test]
fn removed_macro_typed_field_keeps_field_construction() -> Result<(), String> {
    let old = "macro_rules! kind {\n    () => {\n        usize\n    };\n}\npub struct Packet {\n    pub offset: kind!(),\n    pub len: usize,\n}\n";
    removed_only(
        old,
        7,
        "pub offset: kind!(),",
        ProbeFamily::FieldConstruction,
    )?;
    Ok(())
}

#[test]
fn removed_shared_line_keeps_field_construction() -> Result<(), String> {
    let old = "pub struct Packet {\n    pub offset: usize, pub len: usize,\n    pub tag: u8,\n}\n";
    removed_only(
        old,
        2,
        "pub offset: usize, pub len: usize,",
        ProbeFamily::FieldConstruction,
    )?;
    Ok(())
}

#[test]
fn edited_field_keeps_before_after_pair() -> Result<(), String> {
    let mut ops = removed_ops(STRUCT_OLD, 0);
    ops[1] = Op::Replace("    pub offset: u64,");
    let found = findings_for(STRUCT_OLD, &ops, "pub offset: usize,")?;
    if found.len() != 1 {
        return Err(format!("edited field must pair once, got {found:?}"));
    }
    let probe = &found[0].probe;
    if probe.family != ProbeFamily::StaticUnknown
        || probe.after.as_deref().map(str::trim) != Some("pub offset: u64,")
    {
        return Err(format!("edited field lost its pair: {probe:?}"));
    }
    require_declaration_unknown(&found[0])
}

const MACRO_STRUCT_OLD: &str = "pin_project! {\n    pub struct Task<B> {\n        rx: Rx<B>,\n        buffered: Option<Box<dyn Fn(u8) + Send>>,\n        value: compute(),\n    }\n}\n";

/// #10268: a removed field of a struct declared inside an item macro is old
/// source type syntax, judged by the same conservative token-tree recognition.
#[test]
fn removed_only_item_macro_struct_field_is_static_unknown() -> Result<(), String> {
    for (line, text) in [
        (3, "rx: Rx<B>,"),
        (4, "buffered: Option<Box<dyn Fn(u8) + Send>>,"),
    ] {
        let finding = removed_only(MACRO_STRUCT_OLD, line, text, ProbeFamily::StaticUnknown)?;
        require_declaration_unknown(&finding)?;
    }
    Ok(())
}

#[test]
fn removed_item_macro_field_holding_a_call_keeps_field_construction() -> Result<(), String> {
    removed_only(
        MACRO_STRUCT_OLD,
        5,
        "value: compute(),",
        ProbeFamily::FieldConstruction,
    )?;
    Ok(())
}

const MACRO_SOURCE: &str = r###"use std::io::Cursor;
macro_rules! wrap { ($i:item) => { $i }; }
wrap! {
    pub struct Wrapped {
        a: u8,
        buffered: Option<Cursor<Box<[u8]>>>,
        b: Option<u8>,
    }
}
pub fn make() -> Wrapped {
    Wrapped {
        a: 1,
        buffered: None,
        b: None,
    }
}
#[test]
fn observes_construction() {
    let value = make();
    assert_eq!(value.a, 1);
    assert!(value.buffered.is_none());
    assert_eq!(value.b, None);
}
"###;
const MACRO_FIELD: &str = "buffered: Option<Cursor<Box<[u8]>>>,";

#[test]
fn removed_item_macro_field_uses_old_declaration_context() -> Result<(), String> {
    let finding = removed_only(MACRO_SOURCE, 6, MACRO_FIELD, ProbeFamily::StaticUnknown)?;
    require_declaration_unknown(&finding)
}

#[test]
fn added_item_macro_field_is_visible_static_unknown() -> Result<(), String> {
    let old = MACRO_SOURCE.replace("        buffered: Option<Cursor<Box<[u8]>>>,\n", "");
    let mut ops = removed_ops(&old, 0);
    ops[4] = Op::AddAfter("        buffered: Option<Cursor<Box<[u8]>>>,");
    let findings = TempRepo::create(&old, &ops)?.findings()?;
    assert_eq!(findings.len(), 1, "one added declaration: {findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.probe.family, ProbeFamily::StaticUnknown);
    assert!(finding.probe.before.is_none());
    assert_eq!(finding.probe.after.as_deref(), Some(MACRO_FIELD));
    require_declaration_unknown(finding)
}

#[test]
fn replaced_item_macro_field_keeps_both_sides() -> Result<(), String> {
    let mut ops = removed_ops(MACRO_SOURCE, 0);
    ops[5] = Op::Replace("        buffered: Option<Vec<u8>>,");
    let findings = findings_for(MACRO_SOURCE, &ops, MACRO_FIELD)?;
    assert_eq!(findings.len(), 1, "one paired declaration: {findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.probe.family, ProbeFamily::StaticUnknown);
    assert_eq!(
        finding.probe.after.as_deref(),
        Some("buffered: Option<Vec<u8>>,")
    );
    require_declaration_unknown(finding)
}

#[test]
fn removed_item_macro_body_constructor_keeps_runtime_probe() -> Result<(), String> {
    for (old, line) in [
        (MACRO_SOURCE, 14),
        ("wrap! { fn make() { Wrapped {\n    b: None,\n} } }\n", 2),
    ] {
        let findings = findings_for(old, &removed_ops(old, line), "b: None,")?;
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.probe.family == ProbeFamily::FieldConstruction)
                .count(),
            1
        );
        assert!(
            findings
                .iter()
                .all(|f| f.probe.family != ProbeFamily::StaticUnknown && f.probe.after.is_none())
        );
    }
    Ok(())
}

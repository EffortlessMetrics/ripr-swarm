//! Record-field declarations and initializers require distinct source identities.
//! Regression for EffortlessMetrics/ripr#1453 and ub-review#1306.

use super::diff::probes_for_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{Probe, ProbeFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SOURCE: &str = "struct Marker;\nstruct Packet {\n    value: Marker,\n}\nfn packet() -> Packet {\n    Packet {\n        value: Marker,\n    }\n}\n";

fn probes_at(source: &str, line: usize) -> Result<Vec<Probe>, String> {
    let path = PathBuf::from("src/lib.rs");
    let text = source
        .lines()
        .nth(line.saturating_sub(1))
        .ok_or_else(|| format!("fixture has no line {line}"))?
        .to_string();
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    let index = RustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..RustIndex::default()
    };
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
    assert_eq!(probes[0].line, 3);
    assert_eq!(probes[0].expression, "value: Marker,");
    Ok(())
}

#[test]
fn identical_record_initializer_retains_executable_subject() -> Result<(), String> {
    let probes = probes_at(SOURCE, 7)?;
    assert!(
        probes.iter().any(|probe| {
            probe.family == ProbeFamily::FieldConstruction
                && probe.line == 7
                && probe.expression.contains("value: Marker")
        }),
        "record declaration handling erased an actual initializer: {probes:?}"
    );
    Ok(())
}

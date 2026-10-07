//! #6914: repository inventory keeps one error_variant seam per error
//! constructor, while diff synthesis still reaches the error path from a
//! change to only the `return` line of a multi-line `return`, or only its
//! constructor line.

use super::diff::probes_for_file;
use super::repo::probes_for_repo_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::seam_inventory::inventory_seams_from_index;
use crate::analysis::seams::SeamKind;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{Probe, ProbeFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SOURCE: &str = concat!(
    "pub enum Error { InvalidFormat }\n",
    "pub fn parse(flag: bool) -> Result<(), Error> {\n",
    "    if flag {\n",
    "        return\n",
    "            Result::<(), Error>::Err(Error::InvalidFormat);\n",
    "    }\n",
    "    Ok(())\n",
    "}\n",
);

fn index() -> Result<RustIndex, String> {
    index_of(SOURCE)
}

fn index_of(source: &str) -> Result<RustIndex, String> {
    let path = PathBuf::from("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    Ok(RustIndex::from_owned(
        crate::analysis::facts::OwnedRustIndex {
            files: BTreeMap::from([(path, facts)]),
            ..Default::default()
        },
    ))
}

/// Production RA adapter and added-side diff producer, with only `line`
/// added and the rest of the function as unchanged context.
fn diff_probes_at(line: usize) -> Result<Vec<Probe>, String> {
    let text = SOURCE
        .lines()
        .nth(line.saturating_sub(1))
        .ok_or_else(|| format!("fixture has no line {line}"))?
        .to_string();
    let changed = ChangedFile {
        path: PathBuf::from("src/lib.rs"),
        added_lines: vec![ChangedLine {
            line,
            new_side_line: line,
            text,
        }],
        removed_lines: Vec::new(),
    };
    Ok(probes_for_file(Path::new("."), &changed, &index()?))
}

fn families(probes: &[Probe]) -> Vec<ProbeFamily> {
    probes.iter().map(|probe| probe.family.clone()).collect()
}

#[test]
fn added_return_line_of_multiline_return_keeps_error_path() -> Result<(), String> {
    let probes = diff_probes_at(4)?;
    let found = families(&probes);
    assert!(
        found.contains(&ProbeFamily::ErrorPath),
        "adding `return` turns a discarded error into a returned one: {probes:?}"
    );
    assert!(found.contains(&ProbeFamily::ReturnValue), "{probes:?}");
    Ok(())
}

#[test]
fn changed_constructor_line_of_multiline_return_keeps_error_path() -> Result<(), String> {
    let probes = diff_probes_at(5)?;
    assert!(
        families(&probes).contains(&ProbeFamily::ErrorPath),
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn multiline_return_err_is_one_repository_error_path() -> Result<(), String> {
    let probes = probes_for_repo_file(Path::new("."), Path::new("src/lib.rs"), &index()?);
    let error_paths: Vec<(usize, &str)> = probes
        .iter()
        .filter(|probe| probe.family == ProbeFamily::ErrorPath)
        .map(|probe| (probe.location.line, probe.expression.as_str()))
        .collect();
    assert_eq!(
        error_paths,
        vec![(5, "Result::<(), Error>::Err(Error::InvalidFormat)")],
        "{probes:?}"
    );
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index()?);
    let error_seams: Vec<(usize, usize, &str)> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::ErrorVariant)
        .map(|seam| (seam.display_line(), seam.byte_offset(), seam.expression()))
        .collect();
    // Lines 1-4 are 33 + 48 + 14 + 15 bytes, then 12 spaces of indent.
    assert_eq!(SOURCE.find("Result::<(), Error>::Err"), Some(122));
    assert_eq!(
        error_seams,
        vec![(5, 122, "Result::<(), Error>::Err(Error::InvalidFormat)")],
        "{seams:?}"
    );
    Ok(())
}

#[test]
fn inventory_keeps_one_error_seam_per_constructor() -> Result<(), String> {
    let source = concat!(
        "pub fn parse(s: &str) -> Result<u8, Error> {\n",
        "    if s.is_empty() { return Err(Error::Empty); }\n",
        "    if s.len() > 3 { return (Err(Error::Long)); }\n",
        "    if s == \"y\" { return Err(Error::Bad(s.len())); }\n",
        "    if s == \"t\" { return Err::<u8, Error>(Error::Bad(3)); }\n",
        "    if s == \"z\" { return pick(s, Err(Error::A), Err(Error::B)); }\n",
        "    if s == \"w\" { return wrap(Error::Bad(1)); }\n",
        "    if s == \"x\" { return s.parse::<u8>().map_err(Error::from); }\n",
        "    if s == \"v\" {\n",
        "        return Err(\n",
        "            Error::Bad(4),\n",
        "        );\n",
        "    }\n",
        "    Ok(1)\n",
        "}\n",
    );
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index_of(source)?);
    let error_seams: Vec<(usize, &str)> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::ErrorVariant)
        .map(|seam| (seam.display_line(), seam.expression()))
        .collect();
    assert_eq!(
        error_seams,
        vec![
            // The constructor is kept; its `return` and its payload call go.
            (2, "Err(Error::Empty)"),
            (3, "Err(Error::Long)"),
            (4, "Err(Error::Bad(s.len()))"),
            (5, "Err::<u8, Error>(Error::Bad(3))"),
            // Not an `Err(..)` payload: each constructor is its own behavior.
            (6, "pick(s, Err(Error::A), Err(Error::B))"),
            (6, "Err(Error::A)"),
            (6, "Err(Error::B)"),
            (7, "wrap(Error::Bad(1))"),
            (7, "Error::Bad(1)"),
            // A returned method chain has no constructor shape to keep.
            (8, "return s.parse::<u8>().map_err(Error::from)"),
            // rustfmt's vertical `Err(\n payload,\n)` keeps only `Err(..)`.
            (10, "Err(\n            Error::Bad(4),\n        )"),
        ],
        "{seams:?}"
    );
    Ok(())
}

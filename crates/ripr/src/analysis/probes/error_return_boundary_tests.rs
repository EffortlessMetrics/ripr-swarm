//! #6914: repository inventory keeps one error_variant seam per error
//! constructor, while diff synthesis still reaches the error path from a
//! change to only the `return` line of a multi-line `return`, or only its
//! constructor line.

use super::diff::probes_for_file;
use super::repo::probes_for_repo_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::{ProbeShapeFact, ProbeShapeKind, RustIndex, error_path_twins};
use crate::analysis::seam_inventory::inventory_seams_from_index;
use crate::analysis::seams::{RequiredDiscriminator, SeamKind};
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

fn summary(probes: &[Probe]) -> Vec<(ProbeFamily, usize, &str)> {
    probes
        .iter()
        .map(|probe| {
            (
                probe.family.clone(),
                probe.location.line,
                probe.expression.as_str(),
            )
        })
        .collect()
}

#[test]
fn added_return_line_of_multiline_return_keeps_error_path() -> Result<(), String> {
    // Adding `return` turns a discarded error into a returned one.
    let probes = diff_probes_at(4)?;
    assert_eq!(
        summary(&probes),
        vec![
            (
                ProbeFamily::ErrorPath,
                4,
                "return\n            Result::<(), Error>::Err(Error::InvalidFormat)"
            ),
            (
                ProbeFamily::ReturnValue,
                4,
                "return\n            Result::<(), Error>::Err(Error::InvalidFormat)"
            ),
        ],
        "{probes:?}"
    );
    Ok(())
}

#[test]
fn changed_constructor_line_of_multiline_return_keeps_error_path() -> Result<(), String> {
    let probes = diff_probes_at(5)?;
    assert_eq!(
        summary(&probes),
        vec![
            (
                ProbeFamily::CallDeletion,
                5,
                "Result::<(), Error>::Err(Error::InvalidFormat)"
            ),
            (
                ProbeFamily::ErrorPath,
                5,
                "Result::<(), Error>::Err(Error::InvalidFormat)"
            ),
        ],
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
    let error_seams: Vec<(usize, usize, &str, &RequiredDiscriminator)> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::ErrorVariant)
        .map(|seam| {
            (
                seam.display_line(),
                seam.byte_offset(),
                seam.expression(),
                seam.required_discriminator(),
            )
        })
        .collect();
    // Lines 1-4 are 33 + 48 + 14 + 15 bytes, then 12 spaces of indent.
    assert_eq!(SOURCE.find("Result::<(), Error>::Err"), Some(122));
    assert_eq!(
        error_seams,
        vec![(
            5,
            122,
            "Result::<(), Error>::Err(Error::InvalidFormat)",
            &RequiredDiscriminator::ErrorVariant {
                variant: "Error::InvalidFormat".to_string(),
            },
        )],
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
        "    if s == \"p\" { return Err((Error::Bad(5))); }\n",
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
            // Extra parentheses around the payload do not hide it.
            (14, "Err((Error::Bad(5)))"),
        ],
        "{seams:?}"
    );
    Ok(())
}

/// A generated file with many error returns must not make the twin check
/// quadratic: 100k shapes would take billions of pairwise comparisons, the
/// failure `FileOwnerLookup` already fixed for owner lookup.
#[test]
fn twin_check_stays_linearithmic_on_a_generated_file() {
    let returns = 50_000;
    let line = "return Err(E);\n";
    let source = line.repeat(returns);
    let shapes: Vec<ProbeShapeFact> = (0..returns)
        .flat_map(|index| {
            let start = index * line.len();
            [
                ProbeShapeFact {
                    start_line: index + 1,
                    end_line: index + 1,
                    start_byte: start,
                    end_byte: start + "return Err(E)".len(),
                    kind: ProbeShapeKind::ErrorPath,
                    text: "return Err(E)".into(),
                },
                ProbeShapeFact {
                    start_line: index + 1,
                    end_line: index + 1,
                    start_byte: start + "return ".len(),
                    end_byte: start + "return Err(E)".len(),
                    kind: ProbeShapeKind::ErrorPath,
                    text: "Err(E)".into(),
                },
            ]
        })
        .collect();
    let twins = error_path_twins(&shapes, &source);
    // Every `return` span is the twin; every constructor stays.
    assert_eq!(twins.iter().filter(|twin| **twin).count(), returns);
    assert!(twins.iter().step_by(2).all(|twin| *twin));
}

/// An ErrorPath shape over `start..end` of a one-line source.
fn error_shape(start: usize, end: usize) -> ProbeShapeFact {
    ProbeShapeFact {
        start_line: 1,
        end_line: 1,
        start_byte: start,
        end_byte: end,
        kind: ProbeShapeKind::ErrorPath,
        text: String::new().into(),
    }
}

/// Every shape in a start group is checked, not only the first, and an
/// `Err(..)` that closes too early does not stop the search for one that
/// closes the payload exactly (#6954).
#[test]
fn twin_check_reads_every_shape_in_a_start_group() {
    // Two spans of `Err((X))` at one start: the first ends one closer short.
    let source = "return Err((X))";
    let shapes = [
        error_shape(0, 15),  // return Err((X))
        error_shape(7, 14),  // Err((X)  -- closes one of two parens
        error_shape(7, 15),  // Err((X))
        error_shape(12, 13), // X
        error_shape(12, 13), // X, the same span again
    ];
    assert_eq!(
        error_path_twins(&shapes, source),
        // The `return` goes; both constructor spans stay; both payload spans go.
        vec![true, false, false, true, true]
    );

    // Two `return` spans around one constructor are both twins.
    let source = "return Err(X)";
    let shapes = [error_shape(0, 13), error_shape(0, 13), error_shape(7, 13)];
    assert_eq!(error_path_twins(&shapes, source), vec![true, true, false]);
}

/// Deep chains that share a start byte must not make the twin check
/// quadratic. Before #6954, 100k shapes at one start took about a minute,
/// and two 20k chains side by side ran past ten minutes; a linear pass takes
/// milliseconds. The bound is coarse so a slow runner cannot trip it.
#[test]
fn twin_check_stays_fast_on_deep_chains_that_share_a_start() {
    let depth = 20_000;
    let link = ".m()";
    let chain = link.repeat(depth);
    // `Err(E.m()…)` closed, then chained again: an outer chain at byte 0
    // whose payload chain starts at byte 4.
    let source = format!("Err(E{chain}){chain}");
    let payload_end = "Err(E".len();
    let outer_end = payload_end + chain.len() + 1;
    let shapes: Vec<ProbeShapeFact> = (0..depth)
        .flat_map(|step| {
            [
                error_shape(0, outer_end + step * link.len()),
                error_shape(4, payload_end + (step + 1) * link.len()),
            ]
        })
        .collect();
    let started = std::time::Instant::now();
    let twins = error_path_twins(&shapes, &source);
    let elapsed = started.elapsed();
    // Only the full payload chain is closed by `Err(..)`'s own paren.
    assert_eq!(twins.iter().filter(|twin| **twin).count(), 1);
    assert_eq!(twins.get(2 * depth - 1), Some(&true));
    assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");

    // The same shape with `return (` in front exercises the other relation.
    let source = format!("return (E{chain}){chain}");
    let inner_start = "return (".len();
    let inner_end = inner_start + 1;
    let return_end = inner_end + chain.len() + 1;
    let shapes: Vec<ProbeShapeFact> = (0..depth)
        .flat_map(|step| {
            [
                error_shape(0, return_end + step * link.len()),
                error_shape(inner_start, inner_end + (step + 1) * link.len()),
            ]
        })
        .collect();
    let started = std::time::Instant::now();
    let twins = error_path_twins(&shapes, &source);
    let elapsed = started.elapsed();
    // Only the shortest `return` wraps the whole chain with closers alone.
    assert_eq!(twins.iter().filter(|twin| **twin).count(), 1);
    assert_eq!(twins.first(), Some(&true));
    assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");
}

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
            // A call wrapping constructors goes; each constructor stays.
            (6, "Err(Error::A)"),
            (6, "Err(Error::B)"),
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

    // The `return` is matched against the inner shape that ends last: the
    // shorter `Err(X)` leaves `(y)` behind, the longer span leaves nothing.
    let source = "return Err(X)(y)";
    let shapes = [error_shape(0, 16), error_shape(7, 13), error_shape(7, 16)];
    assert_eq!(error_path_twins(&shapes, source), vec![true, false, false]);

    // An annotating chain on `Err(..)` makes the `return` a twin; an unknown
    // method or a range does not.
    let source = "return Err(X).context(m)";
    let shapes = [error_shape(0, 24), error_shape(7, 13)];
    assert_eq!(error_path_twins(&shapes, source), vec![true, false]);
    let source = "return Err(X).m()";
    let shapes = [error_shape(0, 17), error_shape(7, 13)];
    assert_eq!(error_path_twins(&shapes, source), vec![false, false]);
    let source = "return Err(X)..y";
    let shapes = [error_shape(0, 16), error_shape(7, 13)];
    assert_eq!(error_path_twins(&shapes, source), vec![false, false]);
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
    // whose payload chain starts at byte 4. The parser gives method calls no
    // ErrorPath shape, so these spans are synthetic; a parsed run of
    // same-start ErrorPath shapes comes from calls on calls
    // (`f(Err(E))(a)(b)…`). The test bounds the algorithm, not a parse.
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
    // The returns grow by ` + 1`, so only the shortest is the inner chain's
    // twin.
    let sums = " + 1".repeat(depth);
    let source = format!("return (E{chain}){sums}");
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

/// #6935: a `return` around a method chain on an error constructor is the
/// constructor's twin, so only `Err(..)` keeps a seam. A returned chain with
/// no constructor inside keeps its own.
#[test]
fn inventory_drops_a_return_around_a_method_chain_on_a_constructor() -> Result<(), String> {
    let source = concat!(
        "pub fn load(p: &str) -> anyhow::Result<u8> {\n",
        "    if p.is_empty() { return Err(Error::Empty).context(\"empty\"); }\n",
        "    if p == \"x\" { return Err(Error::Bad).into(); }\n",
        "    if p == \"y\" { return (Err(Error::Paren)).into(); }\n",
        "    if p == \"z\" {\n",
        "        return Err(Error::Chain)\n",
        "            .context(\"z\");\n",
        "    }\n",
        "    if p == \"w\" { return x.map_err(Error::from); }\n",
        "    Ok(1)\n",
        "}\n",
    );
    let index = index_of(source)?;
    let file = PathBuf::from("src/lib.rs");
    let seams = inventory_seams_from_index(std::slice::from_ref(&file), &index);
    let error_seams: Vec<(usize, &str)> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::ErrorVariant)
        .map(|seam| (seam.display_line(), seam.expression()))
        .collect();
    assert_eq!(
        error_seams,
        vec![
            (2, "Err(Error::Empty)"),
            (3, "Err(Error::Bad)"),
            (4, "Err(Error::Paren)"),
            (6, "Err(Error::Chain)"),
            (9, "return x.map_err(Error::from)"),
        ],
        "{seams:?}"
    );
    Ok(())
}

/// #6938: a call that takes an error constructor as an argument is that
/// constructor's twin, so only the constructor keeps a seam.
#[test]
fn inventory_drops_a_call_wrapping_an_error_constructor() -> Result<(), String> {
    let source = concat!(
        "pub fn poll(s: &str) -> Poll<Result<u8, Error>> {\n",
        "    if s == \"a\" { return Poll::Ready(Err(Error::A)); }\n",
        "    if s == \"b\" { return Ok(Err(Error::B)); }\n",
        "    if s == \"c\" { return wrap::<u8>(s, Err(Error::C)); }\n",
        "    if s == \"d\" { return Error::Outer(Error::Inner(1)); }\n",
        "    if s == \"e\" { return wrap(x.m(s, Err(Error::E))); }\n",
        "    if s == \"f\" { return wrap(m.f(\")\", Err(Error::F))); }\n",
        "    if s == \"g\" { return Ok(Err(Err(Error::G))); }\n",
        "    Poll::Ready(Err(Error::Tail))\n",
        "}\n",
    );
    let index = index_of(source)?;
    let file = PathBuf::from("src/lib.rs");
    let seams = inventory_seams_from_index(std::slice::from_ref(&file), &index);
    let error_seams: Vec<(usize, &str)> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::ErrorVariant)
        .map(|seam| (seam.display_line(), seam.expression()))
        .collect();
    assert_eq!(
        error_seams,
        vec![
            (2, "Err(Error::A)"),
            (3, "Err(Error::B)"),
            (4, "Err(Error::C)"),
            // A capitalised callee builds its own error around the inner one.
            (5, "Error::Outer(Error::Inner(1))"),
            (5, "Error::Inner(1)"),
            // The constructor is not a top-level argument of `wrap`.
            (6, "wrap(x.m(s, Err(Error::E)))"),
            (6, "Err(Error::E)"),
            // A string literal may hide a delimiter, so both shapes stay.
            (7, "wrap(m.f(\")\", Err(Error::F)))"),
            (7, "Err(Error::F)"),
            // `Err(..)` is never a wrapper: its payload rule keeps the outer.
            (8, "Err(Err(Error::G))"),
            (9, "Err(Error::Tail)"),
        ],
        "{seams:?}"
    );
    Ok(())
}

/// Review of #6935/#6938: a wrapper or chain that adds error behavior of its
/// own keeps its seam, and nested wrappers around one `Err(..)` all go.
#[test]
fn inventory_keeps_wrappers_that_add_error_behavior() -> Result<(), String> {
    let source = concat!(
        "pub fn run(s: &str) -> Result<u8, Error> {\n",
        "    if s == \"a\" { return load(s, Error::Strict).map_err(Error::Io); }\n",
        "    if s == \"b\" { return Error::wrap(Error::A(1)).into(); }\n",
        "    if s == \"c\" { return io::Error::new(Kind::Bad, Error::W(1)); }\n",
        "    if s == \"d\" { return Ok(Poll::Ready(Err(Error::N))); }\n",
        "    if s == \"e\" { return Poll::Ready(Err::<u8, Error>(Error::T)); }\n",
        "    if s == \"f\" { return g(s, |e| Err(e)); }\n",
        "    if s == \"h\" { return wrap(')', Err(Error::H)); }\n",
        "    if s == \"i\" { return Err(Error::I)?.into(); }\n",
        "    if s == \"j\" { return (wrap(Error::J), Err(Error::K)); }\n",
        "    if s == \"k\" { return Err(Error::L).map_err(|_| Error::B); }\n",
        "    if s == \"l\" { let v = Error::Outer(Err(Error::C)); }\n",
        "    if s == \"m\" { return Ok(wrap(Err(Error::M)).map_err(convert)); }\n",
        "    if s == \"n\" { return Err(Error::O).or_else::<Error, _>(recover); }\n",
        "    if s == \"o\" { let v = Self::Outer(Err(Error::P)); }\n",
        "    if s == \"p\" { return Err(Error::Q).context(Error::Ctx); }\n",
        "    if s == \"q\" { return Box::new(Err(Error::R)); }\n",
        "    if s == \"r\" { return Err(Error::S).recover(); }\n",
        "    if s == \"t\" { return Err(Error::U).context(\"a ) b\").inspect_err(log); }\n",
        "    if s == \"u\" { return Err(Error::V).context(c) /* ( */ .map_err(|_| B); }\n",
        "    if s == \"v\" { return Ready(Err(Error::W)); }\n",
        "    if s == \"w\" { return Err(Error::X).context('(').map_err(|_| B); }\n",
        "    if s == \"x\" { return Err(Error::Y).context(r#\"\"(\"#).map_err(|_| B); }\n",
        "    wrap(Err(Error::Tail)).map_err(|_| Error::Other)\n",
        "}\n",
    );
    let index = index_of(source)?;
    let file = PathBuf::from("src/lib.rs");
    let seams = inventory_seams_from_index(std::slice::from_ref(&file), &index);
    let error_seams: Vec<(usize, &str)> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::ErrorVariant)
        .map(|seam| (seam.display_line(), seam.expression()))
        .collect();
    assert_eq!(
        error_seams,
        vec![
            // A chain on a non-`Err` shape adds a conversion.
            (2, "return load(s, Error::Strict).map_err(Error::Io)"),
            (2, "load(s, Error::Strict)"),
            // Functions on a type build their own error; the `.into()` on a
            // non-`Err` shape keeps the `return` too.
            (3, "return Error::wrap(Error::A(1)).into()"),
            (3, "Error::wrap(Error::A(1))"),
            (3, "Error::A(1)"),
            (4, "io::Error::new(Kind::Bad, Error::W(1))"),
            (4, "Error::W(1)"),
            // Nested wrappers around one `Err(..)` all go.
            (5, "Err(Error::N)"),
            (6, "Err::<u8, Error>(Error::T)"),
            // A closure argument is not a top-level constructor.
            (7, "g(s, |e| Err(e))"),
            (7, "Err(e)"),
            // A char literal may hide a delimiter.
            (8, "wrap(')', Err(Error::H))"),
            (8, "Err(Error::H)"),
            // `?` is not a method chain on the constructor.
            (9, "return Err(Error::I)?.into()"),
            (9, "Err(Error::I)"),
            // `wrap` holds no error shape; the tuple is not a call.
            (10, "return (wrap(Error::J), Err(Error::K))"),
            (10, "wrap(Error::J)"),
            (10, "Err(Error::K)"),
            // A chain that replaces the error keeps the `return`.
            (11, "return Err(Error::L).map_err(|_| Error::B)"),
            (11, "Err(Error::L)"),
            // A callee naming an error type builds an error around `Err(..)`.
            (12, "Error::Outer(Err(Error::C))"),
            (12, "Err(Error::C)"),
            // `Ok(..)` holds a converted error, not the bare call.
            (13, "Ok(wrap(Err(Error::M)).map_err(convert))"),
            (13, "Err(Error::M)"),
            // A turbofish `or_else` replaces the error too.
            (14, "return Err(Error::O).or_else::<Error, _>(recover)"),
            (14, "Err(Error::O)"),
            // Only known pure wrappers drop around `Err(..)`.
            (15, "Self::Outer(Err(Error::P))"),
            (15, "Err(Error::P)"),
            // `.context(..)` keeps the constructor's error.
            (16, "Err(Error::Q)"),
            (17, "Err(Error::R)"),
            // An unknown method may replace the error; fail closed.
            (18, "return Err(Error::S).recover()"),
            (18, "Err(Error::S)"),
            // Annotating methods keep the error, even with `)` in a message.
            (19, "Err(Error::U)"),
            // A comment in the chain fails closed.
            (
                20,
                "return Err(Error::V).context(c) /* ( */ .map_err(|_| B)"
            ),
            (20, "Err(Error::V)"),
            // A bare `Ready` may be the user's own variant.
            (21, "Ready(Err(Error::W))"),
            (21, "Err(Error::W)"),
            // A char literal in the chain fails closed.
            (22, "return Err(Error::X).context('(').map_err(|_| B)"),
            (22, "Err(Error::X)"),
            // A raw string in the chain fails closed.
            (
                23,
                "return Err(Error::Y).context(r#\"\"(\"#).map_err(|_| B)"
            ),
            (23, "Err(Error::Y)"),
            // The tail chain adds a conversion; only the call goes.
            (24, "wrap(Err(Error::Tail)).map_err(|_| Error::Other)"),
            (24, "Err(Error::Tail)"),
        ],
        "{seams:?}"
    );
    Ok(())
}

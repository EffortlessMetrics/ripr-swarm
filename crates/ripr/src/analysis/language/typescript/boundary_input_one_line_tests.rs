//! One-line function boundary-parameter derivation (#11487 residual).

use super::*;
use std::path::Path;

const ONE_LINE_CHANGED: &str =
    "export function disc(a: number, t: number) { if (a >= t) { return 10 } return 0 }";
const MULTI_LINE_CHANGED: &str = "  if (a >= t) {";

fn parameters_for_line(
    source: &str,
    owner_name: &str,
    changed_line: &str,
) -> Option<Option<TypeScriptBoundaryParameters>> {
    let owners = extract_owners(Path::new("src/m.ts"), source);
    let owner = owners.iter().find(|owner| owner.name == owner_name)?;
    let line = source
        .lines()
        .position(|text| text.trim_end() == changed_line.trim_end())?
        + 1;
    Some(ts_boundary_parameters_in_source(
        source,
        line,
        changed_line,
        owner,
    ))
}

fn one_line_module(body_prefix: &str) -> String {
    format!("{body_prefix}{ONE_LINE_CHANGED}\n")
}

fn multi_line_module(body_prefix: &str) -> String {
    format!(
        "{body_prefix}export function disc(a: number, t: number) {{\n{MULTI_LINE_CHANGED}\n    return 10;\n  }}\n  return 0;\n}}\n"
    )
}

#[test]
fn one_line_and_multi_line_parameter_pairs_match() {
    let expected = Some(TypeScriptBoundaryParameters {
        parameter: "a".to_string(),
        index: 0,
        operand: "t".to_string(),
        operand_index: 1,
    });
    let one_line = one_line_module("");
    let multi_line = multi_line_module("");
    assert_eq!(
        parameters_for_line(&one_line, "disc", ONE_LINE_CHANGED).flatten(),
        expected,
    );
    assert_eq!(
        parameters_for_line(&multi_line, "disc", MULTI_LINE_CHANGED).flatten(),
        expected,
    );
    assert_eq!(
        parameters_for_line(&one_line, "disc", ONE_LINE_CHANGED)
            .flatten()
            .map(|fact| fact.evidence_line()),
        parameters_for_line(&multi_line, "disc", MULTI_LINE_CHANGED)
            .flatten()
            .map(|fact| fact.evidence_line()),
    );
}

#[test]
fn one_line_parameter_reassignment_stays_unresolved() {
    let changed = ONE_LINE_CHANGED.replace("{ if (a >= t)", "{ t = t + 1; if (a >= t)");
    let source = format!("{changed}\n");
    assert_eq!(
        parameters_for_line(&source, "disc", &changed),
        Some(None),
        "rebinding t before the comparison must fail closed",
    );
}

//! Predicate boundary witness through a free function call (#16496).
//!
//! The asserted value must be the whole owner call, or a modeled
//! value-preserving form of it. A comparison, arithmetic, a negation or a
//! template around the call can map both sides of the boundary to one passing
//! value, and a call written only in a comment is not a call. Each case relates
//! the test through the production relation index and checks the relation
//! before the activation.

use super::*;
use std::collections::HashMap;

const OWNER_FILE: &str = "src/discount.ts";
const TEST_FILE: &str = "tests/discount.test.ts";
const OWNER_SOURCE: &str = concat!(
    "export function discount(total: number): number {\n",
    "  if (total > 100) {\n",
    "    return total - 10;\n",
    "  }\n",
    "  return total;\n",
    "}\n",
);
const CHANGED_LINE: &str = "  if (total > 100) {";
const NAMED_IMPORT: &str = "import { discount } from '../src/discount';";
const NAMESPACE_IMPORT: &str = "import * as d from '../src/discount';";

struct Row {
    relation: TypeScriptRelationKind,
    activation: TypeScriptPredicateActivation,
}

/// Relates the one test of `imports` plus `body` to `discount` and evaluates
/// its boundary row.
fn evaluate(imports: &str, body: &str, label: &str) -> Result<Row, String> {
    let test_source = format!(
        "import {{ test, expect }} from 'vitest';\n{imports}\ntest('row', async () => {{\n{body}\n}});\n"
    );
    let files: [(&str, &str); 2] = [(OWNER_FILE, OWNER_SOURCE), (TEST_FILE, &test_source)];
    let workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    let sources: HashMap<PathBuf, String> = files
        .iter()
        .map(|(p, s)| (PathBuf::from(p), (*s).to_string()))
        .collect();
    let index = ReExportIndex::build(&workspace_files, &sources, Path::new(""), None, |path| {
        path.starts_with("tests")
    });
    let owner = extract_owners(Path::new(OWNER_FILE), OWNER_SOURCE)
        .into_iter()
        .find(|owner| owner.name == "discount")
        .ok_or("fixture owner `discount` not extracted")?;
    if OWNER_SOURCE.lines().nth(1) != Some(CHANGED_LINE) {
        return Err("fixture changed line moved".to_string());
    }
    let tests = extract_tests(Path::new(TEST_FILE), &test_source);
    if tests.len() != 1 {
        return Err(format!(
            "{label}: expected one parsed test, got {}",
            tests.len()
        ));
    }
    let relation_index = RelationIndex::build(&tests, &index);
    let candidates = related_test_candidates(&owner, &tests, None, &index, None, &relation_index);
    let candidate = candidates
        .first()
        .ok_or_else(|| format!("{label}: the test must be related"))?;
    let shape = classify_probe_shape_detail(CHANGED_LINE);
    let boundary = ts_predicate_boundary(&shape, CHANGED_LINE);
    let context = TsBoundaryRowContext {
        boundary: &boundary,
        boundary_fact: None,
        line_text: CHANGED_LINE,
        source_line: CHANGED_LINE,
        owner: &owner,
        sibling_tests: &tests,
        alias_map: None,
        workspace_root: None,
        pin_facts: None,
        equality_negation: false,
        reexport_index: Some(&index),
    };
    Ok(Row {
        relation: candidate.relation,
        activation: ts_predicate_boundary_row_activation(&context, candidate),
    })
}

use TypeScriptPredicateActivation as A;
use TypeScriptRelationKind as R;

#[test]
fn the_whole_call_and_its_value_preserving_forms_witness() -> Result<(), String> {
    for (label, imports, body) in [
        ("whole", NAMED_IMPORT, "  expect(discount(100)).toBe(100);"),
        (
            "one-hop local",
            NAMED_IMPORT,
            "  const r = discount(100);\n  expect(r).toBe(100);",
        ),
        (
            "parenthesized",
            NAMED_IMPORT,
            "  expect((discount(100))).toBe(100);",
        ),
        (
            "awaited",
            NAMED_IMPORT,
            "  expect(await discount(100)).toBe(100);",
        ),
        (
            "promise",
            NAMED_IMPORT,
            "  await expect(Promise.resolve(discount(100))).resolves.toBe(100);",
        ),
        (
            "namespace member",
            NAMESPACE_IMPORT,
            "  expect(d.discount(100)).toBe(100);",
        ),
        (
            "comment beside the call",
            NAMED_IMPORT,
            "  expect(discount(100) /* boundary */).toBe(100);",
        ),
    ] {
        let row = evaluate(imports, body, label)?;
        assert!(
            matches!(row.relation, R::DirectOwnerCall | R::ImportedOwnerCall),
            "{label}: unexpected relation {:?}",
            row.relation
        );
        assert_eq!(row.activation, A::Witnessed, "{label}");
    }
    Ok(())
}

/// The boundary input reaches the owner, but the asserted value is not the
/// owner result: present, not discriminated.
#[test]
fn an_operator_around_the_call_does_not_witness() -> Result<(), String> {
    for (label, imports, body) in [
        (
            "comparison",
            NAMED_IMPORT,
            "  expect(discount(100) > 0).toBe(true);",
        ),
        (
            "strict equality",
            NAMED_IMPORT,
            "  expect(discount(100) === 100).toBe(true);",
        ),
        (
            "arithmetic",
            NAMED_IMPORT,
            "  expect(discount(100) + 0).toBe(100);",
        ),
        (
            "negation",
            NAMED_IMPORT,
            "  expect(!discount(100)).toBe(false);",
        ),
        (
            "template",
            NAMED_IMPORT,
            "  expect(`${discount(100)}`).toBe('100');",
        ),
        (
            "namespace comparison",
            NAMESPACE_IMPORT,
            "  expect(d.discount(100) > 0).toBe(true);",
        ),
    ] {
        let row = evaluate(imports, body, label)?;
        assert_eq!(row.activation, A::ReachedWithoutDiscriminator, "{label}");
    }
    Ok(())
}

/// A call spelled only inside a comment never supplies the boundary input.
#[test]
fn a_call_written_only_in_a_comment_does_not_witness() -> Result<(), String> {
    for (label, body) in [
        (
            "block comment",
            "  expect(discount(50) /* discount(100) */).toBe(50);",
        ),
        (
            "argument comment",
            "  expect(discount(/* 100 */ 50)).toBe(50);",
        ),
        (
            "line comment",
            "  expect(discount(50) // discount(100)\n  ).toBe(50);",
        ),
    ] {
        let row = evaluate(NAMED_IMPORT, body, label)?;
        assert_ne!(row.activation, A::Witnessed, "{label}");
    }
    Ok(())
}

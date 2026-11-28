//! Predicate boundary witness through a re-export chain (#16470).
//!
//! A test that calls the owner under a barrel alias (`import { discount as d }
//! from '../src'`) or a renamed re-export (`export { discount as applyDiscount }
//! from './price'`) reaches it through the `ReExportChainFollowed` relation.
//! The boundary witness reads the same locals as owner spellings, so an exact
//! boundary assertion witnesses as it does for a direct import. Each case builds
//! the index with `ReExportIndex::build` over real sources, relates the test
//! through the production relation index, and checks the relation before the
//! activation.

use super::*;
use std::collections::HashMap;

const OWNER_FILE: &str = "src/price.ts";
const TEST_FILE: &str = "tests/price.test.ts";
const OWNER_SOURCE: &str = concat!(
    "export function discount(total: number): number {\n",
    "  if (total >= 100) {\n",
    "    return total * 0.9;\n",
    "  }\n",
    "  return total;\n",
    "}\n",
);
/// The same owner with an optional second parameter, so a nested call can sit
/// in an argument position the owner accepts.
const TWO_PARAMETER_OWNER_SOURCE: &str = concat!(
    "export function discount(total: number, extra?: number): number {\n",
    "  if (total >= 100) {\n",
    "    return total * 0.9;\n",
    "  }\n",
    "  return total;\n",
    "}\n",
);
const CHANGED_LINE: &str = "  if (total >= 100) {";
const BARREL: &str = "export { discount } from './price';\n";
const RENAMED: &str = "export { discount as applyDiscount } from './price';\n";

struct Row {
    relation: TypeScriptRelationKind,
    with_index: TypeScriptPredicateActivation,
    without_index: TypeScriptPredicateActivation,
}

/// Relates the one test in `test_source` to `discount` through the files in
/// `extra` plus the owner, and evaluates its boundary row with and without
/// the re-export index. `None` when the test is not related at all.
fn evaluate(extra: &[(&str, &str)], test_source: &str) -> Result<Option<Row>, String> {
    evaluate_owner(OWNER_SOURCE, extra, test_source)
}

fn evaluate_owner(
    owner_source: &str,
    extra: &[(&str, &str)],
    test_source: &str,
) -> Result<Option<Row>, String> {
    let mut files: Vec<(&str, &str)> = vec![(OWNER_FILE, owner_source)];
    files.extend_from_slice(extra);
    files.push((TEST_FILE, test_source));
    let workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    let sources: HashMap<PathBuf, String> = files
        .iter()
        .map(|(p, s)| (PathBuf::from(p), (*s).to_string()))
        .collect();
    let index = ReExportIndex::build(&workspace_files, &sources, Path::new(""), None, |path| {
        path.starts_with("tests")
    });
    let owner = extract_owners(Path::new(OWNER_FILE), owner_source)
        .into_iter()
        .find(|owner| owner.name == "discount")
        .ok_or("fixture owner `discount` not extracted")?;
    if owner_source.lines().nth(1) != Some(CHANGED_LINE) {
        return Err("fixture changed line moved".to_string());
    }
    let tests = extract_tests(Path::new(TEST_FILE), test_source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    let relation_index = RelationIndex::build(&tests, &index);
    let candidates = related_test_candidates(&owner, &tests, None, &index, None, &relation_index);
    let Some(candidate) = candidates.first() else {
        return Ok(None);
    };
    let shape = classify_probe_shape_detail(CHANGED_LINE);
    let boundary = ts_predicate_boundary(&shape, CHANGED_LINE);
    let activation = |reexport_index: Option<&ReExportIndex>| {
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
            reexport_index,
        };
        ts_predicate_boundary_row_activation(&context, candidate)
    };
    Ok(Some(Row {
        relation: candidate.relation,
        with_index: activation(Some(&index)),
        without_index: activation(None),
    }))
}

fn vitest(imports: &str, body: &str) -> String {
    format!(
        "import {{ test, expect }} from 'vitest';\n{imports}\ntest('row', () => {{\n{body}\n}});\n"
    )
}

use TypeScriptPredicateActivation as A;
use TypeScriptRelationKind as R;

fn related(extra: &[(&str, &str)], source: &str, label: &str) -> Result<Row, String> {
    evaluate(extra, source)?.ok_or_else(|| format!("{label}: the test must be related"))
}

#[test]
fn a_barrel_alias_witnesses_the_exact_boundary() -> Result<(), String> {
    let source = vitest(
        "import { discount as d } from '../src';",
        "  expect(d(100)).toBe(90);",
    );
    let row = related(&[("src/index.ts", BARREL)], &source, "barrel alias")?;
    assert_eq!(row.relation, R::ReExportChainFollowed);
    assert_eq!(row.with_index, A::Witnessed);
    // The index is what names `d` as the owner: without it the row reads
    // only the owner's own name and does not witness.
    assert_ne!(row.without_index, A::Witnessed);
    Ok(())
}

#[test]
fn a_renamed_reexport_witnesses_the_exact_boundary() -> Result<(), String> {
    let source = vitest(
        "import { applyDiscount } from '../src';",
        "  expect(applyDiscount(100)).toBe(90);",
    );
    let row = related(&[("src/index.ts", RENAMED)], &source, "renamed re-export")?;
    assert_eq!(row.relation, R::ReExportChainFollowed);
    assert_eq!(row.with_index, A::Witnessed);
    assert_ne!(row.without_index, A::Witnessed);
    Ok(())
}

#[test]
fn a_two_hop_star_chain_alias_witnesses_the_exact_boundary() -> Result<(), String> {
    let source = vitest(
        "import { discount as d } from '../src';",
        "  expect(d(100)).toBe(90);",
    );
    let files = [
        ("src/mid.ts", BARREL),
        ("src/index.ts", "export * from './mid';\n"),
    ];
    let row = related(&files, &source, "two-hop chain")?;
    assert_eq!(row.relation, R::ReExportChainFollowed);
    assert_eq!(row.with_index, A::Witnessed);
    Ok(())
}

#[test]
fn a_barrel_alias_keeps_the_boundary_guards() -> Result<(), String> {
    let barrel = [("src/index.ts", BARREL)];
    let import = "import { discount as d } from '../src';";
    let off_boundary = related(
        &barrel,
        &vitest(import, "  expect(d(500)).toBe(450);"),
        "off",
    )?;
    assert_eq!(off_boundary.relation, R::ReExportChainFollowed);
    assert_ne!(
        off_boundary.with_index,
        A::Witnessed,
        "an off-boundary input"
    );
    let tautology = related(
        &barrel,
        &vitest(import, "  expect(d(100)).toBe(d(100));"),
        "taut",
    )?;
    assert_eq!(tautology.relation, R::ReExportChainFollowed);
    assert_ne!(
        tautology.with_index,
        A::Witnessed,
        "a self-comparing expectation"
    );
    let renamed = [("src/index.ts", RENAMED)];
    let wrong_name = vitest(
        "import { discount } from '../src';",
        "  expect(discount(100)).toBe(90);",
    );
    assert!(
        evaluate(&renamed, &wrong_name)?.is_none(),
        "the barrel exports only the renamed name"
    );
    Ok(())
}

#[test]
fn a_destructured_shadow_of_a_barrel_local_is_not_related() -> Result<(), String> {
    let source = vitest(
        "import { discount as d } from '../src';\nconst helpers = { d: (n: number) => n };",
        "  const { d } = helpers;\n  expect(d(100)).toBe(100);",
    );
    assert!(
        evaluate(&[("src/index.ts", BARREL)], &source)?.is_none(),
        "`const {{ d }} = helpers` shadows the barrel import"
    );
    Ok(())
}

#[test]
fn an_owner_call_nested_in_a_whole_alias_call_does_not_witness() -> Result<(), String> {
    let barrel = [("src/index.ts", BARREL)];
    let imports =
        "import { discount as d } from '../src';\nimport { discount } from '../src/price';";
    // Control: the two-parameter owner still witnesses a whole alias call.
    let control = evaluate_owner(
        TWO_PARAMETER_OWNER_SOURCE,
        &barrel,
        &vitest(imports, "  expect(d(100)).toBe(90);"),
    )?
    .ok_or("control: the test must be related")?;
    assert_eq!(
        control.with_index,
        A::Witnessed,
        "whole alias call at the boundary"
    );
    // The asserted value is the outer `d(90, ...)`, whose own input is off
    // the boundary; the inner `discount(100)` lands in an ignored argument.
    let nested = evaluate_owner(
        TWO_PARAMETER_OWNER_SOURCE,
        &barrel,
        &vitest(imports, "  expect(d(90, discount(100))).toBe(90);"),
    )?
    .ok_or("nested: the test must be related")?;
    assert_ne!(
        nested.with_index,
        A::Witnessed,
        "an owner call nested in the alias call"
    );
    Ok(())
}

#[test]
fn a_destructured_shadow_in_a_later_declarator_is_not_related() -> Result<(), String> {
    let source = vitest(
        "import { discount as d } from '../src';",
        "  const helpers = { d: (_n: number) => 90 }, { d } = helpers;\n  expect(d(100)).toBe(90);",
    );
    assert!(
        evaluate(&[("src/index.ts", BARREL)], &source)?.is_none(),
        "the second declarator `{{ d }}` shadows the barrel import"
    );
    Ok(())
}

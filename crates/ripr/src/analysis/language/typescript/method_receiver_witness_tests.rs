//! Predicate boundary witness through a method receiver (#16482).
//!
//! A test calls a changed method through a receiver the relation resolves: an
//! inline `new Cart()`, a local bound to one, a renamed or barrel class
//! import, or the class itself for a static. The boundary witness reads the
//! same receivers, so an exact boundary assertion witnesses as it does for a
//! free function. Each case relates the test through the production relation
//! index and checks the relation before the activation.

use super::*;
use std::collections::HashMap;

const OWNER_FILE: &str = "src/cart.ts";
const TEST_FILE: &str = "tests/cart.test.ts";
const OWNER_SOURCE: &str = concat!(
    "export class Cart {\n",
    "  discount(total: number): number {\n",
    "    if (total >= 100) {\n",
    "      return total * 0.9;\n",
    "    }\n",
    "    return total;\n",
    "  }\n",
    "}\n",
);
const STATIC_OWNER_SOURCE: &str = concat!(
    "export class Cart {\n",
    "  static discount(total: number): number {\n",
    "    if (total >= 100) {\n",
    "      return total * 0.9;\n",
    "    }\n",
    "    return total;\n",
    "  }\n",
    "}\n",
);
const CHANGED_LINE: &str = "    if (total >= 100) {";
const IMPORT: &str = "import { Cart } from '../src/cart';";
const OTHER_SOURCE: &str =
    "export class Other {\n  discount(_t: number): number { return 90; }\n}\n";

struct Row {
    relation: TypeScriptRelationKind,
    activation: TypeScriptPredicateActivation,
}

/// Relates the one test in `test_source` to the method `discount` of
/// `owner_source` through the files in `extra` plus the owner, and evaluates
/// its boundary row. `None` when the test is not related at all.
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
    if owner_source.lines().nth(2) != Some(CHANGED_LINE) {
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
    Ok(Some(Row {
        relation: candidate.relation,
        activation: ts_predicate_boundary_row_activation(&context, candidate),
    }))
}

fn vitest(imports: &str, body: &str) -> String {
    format!(
        "import {{ test, expect, vi }} from 'vitest';\n{imports}\ntest('row', () => {{\n{body}\n}});\n"
    )
}

use TypeScriptPredicateActivation as A;
use TypeScriptRelationKind as R;

fn related(extra: &[(&str, &str)], imports: &str, body: &str, label: &str) -> Result<Row, String> {
    evaluate_owner(OWNER_SOURCE, extra, &vitest(imports, body))?
        .ok_or_else(|| format!("{label}: the test must be related"))
}

#[test]
fn an_inline_and_a_bound_receiver_witness_the_exact_boundary() -> Result<(), String> {
    for (label, body) in [
        ("inline", "  expect(new Cart().discount(100)).toBe(90);"),
        (
            "optional chain",
            "  expect(new Cart()?.discount(100)).toBe(90);",
        ),
        (
            "bound",
            "  const c = new Cart();\n  expect(c.discount(100)).toBe(90);",
        ),
        (
            "bound result",
            "  const c = new Cart();\n  const r = c.discount(100);\n  expect(r).toBe(90);",
        ),
    ] {
        let row = related(&[], IMPORT, body, label)?;
        assert_eq!(row.relation, R::ReceiverOwnerCall, "{label}");
        assert_eq!(row.activation, A::Witnessed, "{label}");
    }
    Ok(())
}

#[test]
fn a_renamed_or_barrel_class_import_witnesses_the_exact_boundary() -> Result<(), String> {
    let renamed = related(
        &[],
        "import { Cart as Basket } from '../src/cart';",
        "  const c = new Basket();\n  expect(c.discount(100)).toBe(90);",
        "renamed",
    )?;
    assert_eq!(renamed.relation, R::ReceiverOwnerCall);
    assert_eq!(renamed.activation, A::Witnessed);
    let barrel = related(
        &[("src/index.ts", "export { Cart } from './cart';\n")],
        "import { Cart } from '../src';",
        "  expect(new Cart().discount(100)).toBe(90);",
        "barrel",
    )?;
    assert_eq!(barrel.relation, R::ReExportChainFollowed);
    assert_eq!(barrel.activation, A::Witnessed);
    Ok(())
}

#[test]
fn a_static_method_witnesses_through_its_class() -> Result<(), String> {
    let row = evaluate_owner(
        STATIC_OWNER_SOURCE,
        &[],
        &vitest(IMPORT, "  expect(Cart.discount(100)).toBe(90);"),
    )?
    .ok_or("static: the test must be related")?;
    assert_eq!(row.relation, R::ClassMethodCall);
    assert_eq!(row.activation, A::Witnessed);
    Ok(())
}

#[test]
fn a_method_receiver_keeps_the_boundary_guards() -> Result<(), String> {
    for (label, body) in [
        (
            "off boundary",
            "  expect(new Cart().discount(500)).toBe(450);",
        ),
        (
            "self-comparing",
            "  const c = new Cart();\n  expect(c.discount(100)).toBe(c.discount(100));",
        ),
        // The boundary input goes to the constructor, not the method.
        (
            "constructor argument",
            "  expect(new Cart(100).discount(5)).toBe(5);",
        ),
        (
            "nested call",
            "  const c = new Cart();\n  expect(c.discount(90 + 0 * c.discount(100))).toBe(90);",
        ),
        // The asserted value must be the whole method call: a comparison or
        // an arithmetic wrapper can map both sides of the boundary to one
        // passing value, and a call spelled in a comment is not a call.
        (
            "comparison",
            "  const c = new Cart();\n  expect(c.discount(100) > 0).toBe(true);",
        ),
        (
            "call in a comment",
            "  expect(new Cart().discount(50) /* new Cart().discount(100) */ + 40).toBe(90);",
        ),
        // The initializer must be one `new Cart(...)`: a comma expression
        // leaves `c` holding the last value.
        (
            "comma initializer",
            "  const c = (new Cart(), { discount: (_n: number) => 90 });\n  new Cart().discount(1);\n  expect(c.discount(100)).toBe(90);",
        ),
        // A test that spells the placeholder receiver names its own object.
        (
            "placeholder spelling",
            "  new Cart().discount(1);\n  const $riprNew$ = { discount: (_n: number) => 90 };\n  expect($riprNew$.discount(100)).toBe(90);",
        ),
        (
            "unrelated receiver",
            "  const c = new Cart();\n  c.discount(1);\n  const o = { discount: (_n: number) => 90 };\n  expect(o.discount(100)).toBe(90);",
        ),
    ] {
        let row = related(&[], IMPORT, body, label)?;
        assert_ne!(row.activation, A::Witnessed, "{label}");
    }
    let other_class = related(
        &[("src/other.ts", OTHER_SOURCE)],
        "import { Cart } from '../src/cart';\nimport { Other } from '../src/other';",
        "  new Cart().discount(1);\n  expect(new Other().discount(100)).toBe(90);",
        "other class",
    )?;
    assert_ne!(
        other_class.activation,
        A::Witnessed,
        "a same-named method of another class"
    );
    // The block-local `cart` is another object at the asserted call.
    let shadowed_receiver = related(
        &[("src/other.ts", OTHER_SOURCE)],
        "import { Cart } from '../src/cart';\nimport { Other } from '../src/other';",
        "  const cart = new Cart();\n  cart.discount(50);\n  {\n    const cart = new Other();\n    expect(cart.discount(100)).toBe(90);\n  }",
        "shadowed receiver",
    )?;
    assert_ne!(
        shadowed_receiver.activation,
        A::Witnessed,
        "a receiver name declared twice"
    );
    Ok(())
}

#[test]
fn a_replaced_spied_or_shadowed_method_is_not_related() -> Result<(), String> {
    for (label, body) in [
        (
            "rebound",
            "  let c = new Cart();\n  c = { discount: (_n: number) => 90 } as any;\n  expect(c.discount(100)).toBe(90);",
        ),
        (
            "replaced",
            "  const c = new Cart();\n  c.discount = (_n: number) => 90;\n  expect(c.discount(100)).toBe(90);",
        ),
        (
            "spied",
            "  vi.spyOn(Cart.prototype, 'discount').mockReturnValue(90);\n  expect(new Cart().discount(100)).toBe(90);",
        ),
        (
            "shadowed class",
            "  class Cart { discount(_n: number) { return 90; } }\n  expect(new Cart().discount(100)).toBe(90);",
        ),
    ] {
        assert!(
            evaluate_owner(OWNER_SOURCE, &[], &vitest(IMPORT, body))?.is_none(),
            "{label}: the test must not be related"
        );
    }
    Ok(())
}

#[test]
fn the_inline_rewrite_keeps_the_method_arguments() {
    use super::inline_new_receiver::{INLINE_NEW_RECEIVER, rewrite_inline_new_receivers};
    let constructors = vec!["Cart".to_string()];
    let rewritten = rewrite_inline_new_receivers("new Cart(1, f(2)).discount(100)", &constructors);
    assert_eq!(rewritten, format!("{INLINE_NEW_RECEIVER}.discount(100)"));
    // Constructor text inside a string or a comment argument is not a receiver.
    assert_eq!(
        rewrite_inline_new_receivers(
            "new Cart().discount(\"new Cart().x\" /* new Cart().y */)",
            &constructors
        ),
        format!("{INLINE_NEW_RECEIVER}.discount(\"new Cart().x\" /* new Cart().y */)")
    );
    for unchanged in [
        "new CartX().discount(100)",
        "x.new Cart().discount(100)",
        "renew Cart().discount(100)",
        "new Cart(",
        "new Cart()",
    ] {
        assert_eq!(
            rewrite_inline_new_receivers(unchanged, &constructors),
            unchanged
        );
    }
}

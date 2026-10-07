//! Row-local predicate-boundary activation controls (#5527).
//!
//! Each case writes a real owner module under a temporary workspace root,
//! parses a real test source, relates the two through the production
//! `related_test_candidates` path, and evaluates every row with
//! `ts_predicate_boundary_row_activation` against the module-derived
//! boundary fact the classifier reads. Cases assert the fixture first (the
//! fact exists or not, each row has the relation the case depends on), then
//! the per-row activation, and finally that the finding-wide witness is the
//! reduction of those rows.

use super::tests::ts_unique_tempdir;
use super::*;

const OWNER_FILE: &str = "src/pricing.ts";
const TEST_FILE: &str = "tests/pricing.test.ts";
const OWNER_SOURCE: &str = concat!(
    "export const FEE_THRESHOLD = 50;\n",
    "export let SOFT_LIMIT = 10;\n",
    "\n",
    "export function applyDiscount(total: number): number {\n",
    "  if (total >= 100) {\n",
    "    return total * 0.9;\n",
    "  }\n",
    "  return total;\n",
    "}\n",
    "\n",
    "export function applyFee(total: number): number {\n",
    "  if (total >= FEE_THRESHOLD) {\n",
    "    return 0;\n",
    "  }\n",
    "  return 5;\n",
    "}\n",
    "\n",
    "export function inRange(amount: number, limit: number): boolean {\n",
    "  if (amount >= limit) {\n",
    "    return true;\n",
    "  }\n",
    "  return false;\n",
    "}\n",
    "\n",
    "export function isSoft(total: number): boolean {\n",
    "  if (total >= SOFT_LIMIT) {\n",
    "    return true;\n",
    "  }\n",
    "  return false;\n",
    "}\n",
    "\n",
    "export function tier(total: number): number {\n",
    "  if (total >= 10) {\n",
    "    return 1;\n",
    "  }\n",
    "  return tier(total + 1);\n",
    "}\n",
    "\n",
    "export const SMALL = 50;\n",
    "\n",
    "export function band(total: number): number {\n",
    "  if (total >= 10) {\n",
    "    return 1;\n",
    "  }\n",
    "  return step(total);\n",
    "}\n",
    "\n",
    "function step(total: number): number {\n",
    "  return band(total + 5);\n",
    "}\n",
);

/// A changed line of `OWNER_SOURCE`, with the owner it belongs to.
struct Change {
    owner: &'static str,
    line: usize,
    text: &'static str,
}

const DISCOUNT: Change = Change {
    owner: "applyDiscount",
    line: 5,
    text: "  if (total >= 100) {",
};
const FEE: Change = Change {
    owner: "applyFee",
    line: 12,
    text: "  if (total >= FEE_THRESHOLD) {",
};
const RANGE: Change = Change {
    owner: "inRange",
    line: 19,
    text: "  if (amount >= limit) {",
};
const SOFT: Change = Change {
    owner: "isSoft",
    line: 26,
    text: "  if (total >= SOFT_LIMIT) {",
};

const TIER: Change = Change {
    owner: "tier",
    line: 33,
    text: "  if (total >= 10) {",
};

const BAND: Change = Change {
    owner: "band",
    line: 42,
    text: "  if (total >= 10) {",
};

/// One evaluated finding: each row's test name, relation and activation,
/// in candidate order, plus the finding-wide witness.
struct Evaluated {
    rows: Vec<(
        String,
        TypeScriptRelationKind,
        TypeScriptPredicateActivation,
    )>,
    witnessed: bool,
}

impl Evaluated {
    /// The row recorded for `test`, or an error naming every row present.
    fn row(
        &self,
        test: &str,
    ) -> Result<
        &(
            String,
            TypeScriptRelationKind,
            TypeScriptPredicateActivation,
        ),
        String,
    > {
        self.rows
            .iter()
            .find(|(name, _, _)| name == test)
            .ok_or_else(|| format!("no related row for `{test}`: {:?}", self.rows))
    }

    /// The activation recorded on `test`'s row.
    fn activation(&self, test: &str) -> Result<TypeScriptPredicateActivation, String> {
        Ok(self.row(test)?.2)
    }
}

/// Runs the classifier on `change` against `test_source` in a fresh
/// tempdir, removing the tempdir afterwards.
fn evaluate(change: &Change, test_source: &str, expect_fact: bool) -> Result<Evaluated, String> {
    let root = ts_unique_tempdir("predicate-activation")?;
    let result = evaluate_in(&root, change, test_source, expect_fact);
    let _ = std::fs::remove_dir_all(&root);
    result
}

/// Writes the owner module and test file under `root`, checks the fixture
/// (changed line text and boundary-fact presence), then classifies.
fn evaluate_in(
    root: &Path,
    change: &Change,
    test_source: &str,
    expect_fact: bool,
) -> Result<Evaluated, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir src: {err}"))?;
    std::fs::write(root.join(OWNER_FILE), OWNER_SOURCE)
        .map_err(|err| format!("write owner: {err}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|err| format!("mkdir tests: {err}"))?;
    std::fs::write(root.join(TEST_FILE), test_source)
        .map_err(|err| format!("write test file: {err}"))?;
    let owner = extract_owners(Path::new(OWNER_FILE), OWNER_SOURCE)
        .into_iter()
        .find(|owner| owner.name == change.owner)
        .ok_or_else(|| format!("fixture owner `{}` not extracted", change.owner))?;
    let module_line = OWNER_SOURCE.lines().nth(change.line - 1);
    if module_line != Some(change.text) {
        return Err(format!(
            "fixture changed line {} is {module_line:?}, not {:?}",
            change.line, change.text
        ));
    }
    let tests = extract_tests(Path::new(TEST_FILE), test_source);
    if tests.is_empty() {
        return Err(format!("fixture parsed to no tests: {test_source}"));
    }
    let candidates =
        related_test_candidates(&owner, &tests, Some(root), &ReExportIndex::empty(), None);
    let shape = classify_probe_shape_detail(change.text);
    let boundary = ts_predicate_boundary(&shape, change.text);
    let fact = ts_boundary_fact_for_change(&shape, change.line, change.text, &owner, Some(root));
    if fact.is_some() != expect_fact {
        return Err(format!(
            "fixture boundary fact for `{}`: expected present={expect_fact}, got {fact:?}",
            change.text
        ));
    }
    let context = |boundary_fact| TsBoundaryRowContext {
        boundary: &boundary,
        boundary_fact,
        line_text: change.text,
        owner: &owner,
        sibling_tests: &tests,
        alias_map: None,
        workspace_root: Some(root),
    };
    let activations: Vec<TypeScriptPredicateActivation> = candidates
        .iter()
        .map(|candidate| ts_predicate_boundary_row_activation(&context(fact.as_ref()), candidate))
        .collect();
    let witnessed = ts_predicate_boundary_witnessed_by_rows(&boundary, &activations);
    // The fact only splits non-witnessing rows: evaluating without it never
    // moves a row into or out of `Witnessed`.
    for (candidate, activation) in candidates.iter().zip(&activations) {
        let without_fact = ts_predicate_boundary_row_activation(&context(None), candidate);
        assert_eq!(
            without_fact == TypeScriptPredicateActivation::Witnessed,
            *activation == TypeScriptPredicateActivation::Witnessed,
            "`{}`: the boundary fact must not decide the witness",
            candidate.test.name
        );
        assert_ne!(
            without_fact,
            TypeScriptPredicateActivation::MissedBoundary,
            "`{}`: a miss needs the module-derived boundary fact",
            candidate.test.name
        );
    }
    Ok(Evaluated {
        rows: candidates
            .iter()
            .zip(activations)
            .map(|(candidate, activation)| {
                (candidate.test.name.clone(), candidate.relation, activation)
            })
            .collect(),
        witnessed,
    })
}

/// A test file holding `imports` and one `test(name, ...)` with `body`.
fn one_test(name: &str, imports: &str, body: &str) -> String {
    format!("{imports}\ntest('{name}', () => {{\n{body}\n}});\n")
}

const IMPORT_DISCOUNT: &str = "import { applyDiscount } from '../src/pricing';";

use TypeScriptPredicateActivation as A;
use TypeScriptRelationKind as R;

/// Control 1: the exact boundary literal reaches the read parameter.
#[test]
fn exact_boundary_argument_is_witnessed() -> Result<(), String> {
    let source = one_test(
        "at boundary",
        IMPORT_DISCOUNT,
        "  expect(applyDiscount(100)).toBe(90);",
    );
    let evaluated = evaluate(&DISCOUNT, &source, true)?;
    assert_eq!(evaluated.row("at boundary")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("at boundary")?, A::Witnessed);
    assert!(evaluated.witnessed);
    Ok(())
}

/// Control 2: an off-boundary plain integer is a definite miss.
#[test]
fn off_boundary_argument_misses_the_boundary() -> Result<(), String> {
    let source = one_test(
        "above boundary",
        IMPORT_DISCOUNT,
        "  expect(applyDiscount(150)).toBe(135);",
    );
    let evaluated = evaluate(&DISCOUNT, &source, true)?;
    assert_eq!(evaluated.row("above boundary")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("above boundary")?, A::MissedBoundary);
    assert!(!evaluated.witnessed);
    Ok(())
}

/// Control 3: the boundary literal parked in an argument the owner never
/// reads is no activation; the read argument is off the boundary.
#[test]
fn dead_extra_argument_with_the_boundary_literal_misses() -> Result<(), String> {
    let source = one_test(
        "dead argument",
        IMPORT_DISCOUNT,
        "  expect(applyDiscount(150, 100)).toBe(135);",
    );
    let evaluated = evaluate(&DISCOUNT, &source, true)?;
    assert_eq!(evaluated.row("dead argument")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("dead argument")?, A::MissedBoundary);
    assert!(!evaluated.witnessed);
    Ok(())
}

/// Control 4: arithmetic text holding the literal is not folded, so its
/// runtime value stays unknown, never a miss and never a witness.
#[test]
fn nested_arithmetic_argument_is_unresolved() -> Result<(), String> {
    for (name, call) in [
        ("folds to boundary", "applyDiscount(50 + 50)"),
        ("contains boundary text", "applyDiscount(100 + 1)"),
    ] {
        let source = one_test(
            name,
            IMPORT_DISCOUNT,
            &format!("  expect({call}).toBe(90);"),
        );
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{call}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{call}");
        assert!(!evaluated.witnessed, "{call}");
    }
    Ok(())
}

/// Control 5: a same-name method on an unrelated receiver is not the owner,
/// so its boundary literal never witnesses. The miss check cannot prove the
/// receiver is not the owner module either (a `require` namespace looks the
/// same), so the row stays unresolved rather than missed.
#[test]
fn unrelated_receiver_same_name_call_is_not_owner_activation() -> Result<(), String> {
    let source = one_test(
        "other receiver",
        IMPORT_DISCOUNT,
        concat!(
            "  const cart = makeCart();\n",
            "  expect(cart.applyDiscount(100)).toBe(90);\n",
            "  expect(applyDiscount(150)).toBe(135);",
        ),
    );
    let evaluated = evaluate(&DISCOUNT, &source, true)?;
    assert_eq!(evaluated.row("other receiver")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("other receiver")?, A::Unresolved);
    assert!(!evaluated.witnessed);
    Ok(())
}

/// Control 6: a body-local declaration of the owner name shadows it, so a
/// boundary-shaped bare call activates the shadow, not the owner. Alone it
/// is not related at all. Beside a trusted namespace call, the shadowed
/// boundary call cannot donate a witness, and the row stays unresolved; a
/// declaration with no bare use leaves the namespace call's own answer.
#[test]
fn body_local_shadow_is_not_owner_activation() -> Result<(), String> {
    let shadow = "  const applyDiscount = (total: number) => total;\n";
    let bare = one_test(
        "shadowed",
        IMPORT_DISCOUNT,
        &format!("{shadow}  expect(applyDiscount(100)).toBe(100);"),
    );
    let evaluated = evaluate(&DISCOUNT, &bare, true)?;
    assert!(
        evaluated.rows.is_empty(),
        "fixture: a shadowed bare call is not related, got {:?}",
        evaluated.rows
    );
    assert!(!evaluated.witnessed);
    for (name, body, activation) in [
        (
            "shadowed boundary call beside namespace call",
            "  expect(applyDiscount(100)).toBe(100);\n  expect(pricing.applyDiscount(150)).toBe(135);",
            A::Unresolved,
        ),
        (
            "unused shadow beside namespace boundary call",
            "  expect(pricing.applyDiscount(100)).toBe(90);",
            A::Witnessed,
        ),
    ] {
        let source = one_test(
            name,
            "import * as pricing from '../src/pricing';",
            &format!("{shadow}{body}"),
        );
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::ImportedOwnerCall, "{body}");
        assert_eq!(evaluated.activation(name)?, activation, "{body}");
    }
    Ok(())
}

/// Control 7: genuine owner calls through a namespace import or an import
/// alias keep their real answer. The aggregate witness reads only the owner
/// name, so an alias call at the boundary is present but not witnessed, and
/// an alias call can never hide a boundary input from the miss check.
#[test]
fn namespace_and_alias_owner_calls_keep_their_answer() -> Result<(), String> {
    let namespace = "import * as pricing from '../src/pricing';";
    let cases = [
        (
            "namespace at boundary",
            namespace,
            "pricing.applyDiscount(100)",
            R::ImportedOwnerCall,
            A::Witnessed,
        ),
        (
            "namespace off boundary",
            namespace,
            "pricing.applyDiscount(150)",
            R::ImportedOwnerCall,
            A::MissedBoundary,
        ),
    ];
    for (name, imports, call, relation, activation) in cases {
        let source = one_test(name, imports, &format!("  expect({call}).toBe(90);"));
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, relation, "{call}");
        assert_eq!(evaluated.activation(name)?, activation, "{call}");
    }
    let alias = "import { applyDiscount as discount } from '../src/pricing';";
    for (name, call, activation) in [
        (
            "alias at boundary",
            "discount(100)",
            A::ReachedWithoutDiscriminator,
        ),
        ("alias off boundary", "discount(150)", A::MissedBoundary),
    ] {
        let source = one_test(name, alias, &format!("  expect({call}).toBe(90);"));
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::ImportAliasOwnerCall, "{call}");
        assert_eq!(evaluated.activation(name)?, activation, "{call}");
        assert!(!evaluated.witnessed, "{call}");
    }
    Ok(())
}

/// Control 8: the owner module's named constant resolves the boundary, and
/// a test-scope constant argument resolves to its value.
#[test]
fn owner_module_named_constant_resolves_the_boundary() -> Result<(), String> {
    let imports = "import { applyFee, FEE_THRESHOLD } from '../src/pricing';";
    for (name, body, activation) in [
        (
            "imported constant",
            "  expect(applyFee(FEE_THRESHOLD)).toBe(0);",
            A::Witnessed,
        ),
        (
            "local constant",
            "  const LIMIT = 50;\n  expect(applyFee(LIMIT)).toBe(0);",
            A::Witnessed,
        ),
        (
            "literal at constant",
            "  expect(applyFee(50)).toBe(0);",
            A::Witnessed,
        ),
        (
            "off constant",
            "  expect(applyFee(20)).toBe(5);",
            A::MissedBoundary,
        ),
        (
            "constant arithmetic",
            "  expect(applyFee(FEE_THRESHOLD - 1)).toBe(5);",
            A::Unresolved,
        ),
    ] {
        let source = one_test(name, imports, body);
        let evaluated = evaluate(&FEE, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{body}");
        assert_eq!(evaluated.activation(name)?, activation, "{body}");
    }
    Ok(())
}

/// Control 9: a boundary operand the module does not pin (a mutable `let`)
/// leaves an off-boundary-looking call unresolved, never a miss.
#[test]
fn unpinned_boundary_operand_is_unresolved_not_missed() -> Result<(), String> {
    let source = one_test(
        "mutable limit",
        "import { isSoft } from '../src/pricing';",
        "  expect(isSoft(3)).toBe(false);",
    );
    let evaluated = evaluate(&SOFT, &source, false)?;
    assert_eq!(evaluated.row("mutable limit")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("mutable limit")?, A::Unresolved);
    Ok(())
}

/// Two-parameter comparison: equal plain arguments hit the boundary, and
/// distinct plain arguments miss it.
#[test]
fn parameter_pair_boundary_splits_rows() -> Result<(), String> {
    let source = format!(
        "{}\ntest('equal pair', () => {{\n  expect(inRange(7, 7)).toBe(true);\n}});\ntest('distinct pair', () => {{\n  expect(inRange(5, 10)).toBe(false);\n}});\n",
        "import { inRange } from '../src/pricing';"
    );
    let evaluated = evaluate(&RANGE, &source, true)?;
    assert_eq!(evaluated.row("equal pair")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.row("distinct pair")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("equal pair")?, A::Witnessed);
    assert_eq!(evaluated.activation("distinct pair")?, A::MissedBoundary);
    assert!(evaluated.witnessed);
    Ok(())
}

/// Controls 10 and 11: in one finding, the row at the boundary witnesses
/// and the off-boundary row misses; neither donates to the other, and
/// permuting the test order keeps every row's own result.
#[test]
fn mixed_rows_keep_their_own_activation_in_any_order() -> Result<(), String> {
    let at = "test('at boundary', () => {\n  expect(applyDiscount(100)).toBe(90);\n});\n";
    let off = "test('above boundary', () => {\n  expect(applyDiscount(150)).toBe(135);\n});\n";
    let mut seen = Vec::new();
    for order in [[at, off], [off, at]] {
        let source = format!("{IMPORT_DISCOUNT}\n{}{}", order[0], order[1]);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.rows.len(), 2, "fixture: two related rows");
        assert_eq!(evaluated.activation("at boundary")?, A::Witnessed);
        assert_eq!(evaluated.activation("above boundary")?, A::MissedBoundary);
        assert!(evaluated.witnessed);
        let mut by_name: Vec<(String, TypeScriptPredicateActivation)> = evaluated
            .rows
            .iter()
            .map(|(name, _, activation)| (name.clone(), *activation))
            .collect();
        by_name.sort_by(|left, right| left.0.cmp(&right.0));
        seen.push(by_name);
    }
    assert_eq!(
        seen[0], seen[1],
        "row attribution must not depend on test order"
    );
    Ok(())
}

/// A boundary input that reaches the owner without a discriminating
/// assertion is present, so it is never a miss: a weak assertion at the
/// boundary, an unobserved call at the boundary beside an observed
/// off-boundary call, and a dead expected side.
#[test]
fn boundary_input_without_discriminator_is_reached_not_missed() -> Result<(), String> {
    for (name, body) in [
        (
            "weak at boundary",
            "  expect(applyDiscount(100)).toBeTruthy();",
        ),
        (
            "unobserved boundary call",
            "  applyDiscount(100);\n  expect(applyDiscount(150)).toBe(135);",
        ),
        (
            "dead expected side",
            "  expect(applyDiscount(100)).toBe(999);",
        ),
    ] {
        let source = one_test(name, IMPORT_DISCOUNT, body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{body}");
        assert_eq!(
            evaluated.activation(name)?,
            A::ReachedWithoutDiscriminator,
            "{body}"
        );
        assert!(!evaluated.witnessed, "{body}");
    }
    Ok(())
}

/// An owner reference that is not a plain call hides its inputs, so the row
/// stays unresolved even beside an observed off-boundary call.
#[test]
fn non_call_owner_reference_is_unresolved() -> Result<(), String> {
    for (name, extra) in [
        ("callback reference", "  [100].map(applyDiscount);"),
        ("apply call", "  applyDiscount.apply(null, [100]);"),
    ] {
        let body = format!("{extra}\n  expect(applyDiscount(150)).toBe(135);");
        let source = one_test(name, IMPORT_DISCOUNT, &body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{extra}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{extra}");
    }
    Ok(())
}

/// A changed line that is not a predicate has no boundary to activate, and
/// the finding-wide witness does not gate it.
#[test]
fn non_predicate_change_is_not_applicable() -> Result<(), String> {
    let change = Change {
        owner: "applyDiscount",
        line: 6,
        text: "    return total * 0.9;",
    };
    let source = one_test(
        "return value",
        IMPORT_DISCOUNT,
        "  expect(applyDiscount(150)).toBe(135);",
    );
    let evaluated = evaluate(&change, &source, false)?;
    assert_eq!(evaluated.activation("return value")?, A::NotApplicable);
    assert!(evaluated.witnessed);
    Ok(())
}

/// Review B1 (#5527): a boundary input that reaches the owner outside the
/// test body (a hook, a describe-level binding, a test helper, or another
/// production function) is invisible to the body scan, so a row whose
/// assertions observe anything but one owner call stays unresolved.
#[test]
fn input_built_outside_the_body_is_unresolved_not_missed() -> Result<(), String> {
    let cases = [
        (
            "hook value",
            concat!(
                "import { applyDiscount } from '../src/pricing';\n",
                "let atBoundary: number;\n",
                "beforeEach(() => {\n  atBoundary = applyDiscount(100);\n});\n",
            ),
            "  expect(applyDiscount(150)).toBe(135);\n  expect(atBoundary).toBe(90);",
        ),
        (
            "test helper",
            concat!(
                "import { applyDiscount } from '../src/pricing';\n",
                "const discounted = (n: number) => applyDiscount(n);\n",
            ),
            "  expect(applyDiscount(150)).toBe(135);\n  expect(discounted(100)).toBe(90);",
        ),
        (
            "production caller",
            "import { applyDiscount, checkoutTotal } from '../src/pricing';\n",
            "  expect(applyDiscount(150)).toBe(135);\n  expect(checkoutTotal(100)).toBe(90);",
        ),
        (
            "compound observation",
            "import { applyDiscount } from '../src/pricing';\n",
            "  expect(applyDiscount(150) + bonus(100)).toBe(135);",
        ),
    ];
    for (name, imports, body) in cases {
        let source = one_test(name, imports, body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{name}");
    }
    Ok(())
}

/// Review B2 (#5527): a receiver or import that may reach the owner never
/// hides a boundary input. A `require` namespace the resolver places on the
/// owner module counts as an owner call (present, so reached); an alias
/// through a barrel it cannot place refuses the miss check.
#[test]
fn unproven_receiver_or_barrel_import_never_hides_a_boundary_input() -> Result<(), String> {
    let cases = [
        (
            "require namespace",
            "import { applyDiscount } from '../src/pricing';\nconst pricing = require('../src/pricing');\n",
            "  expect(applyDiscount(150)).toBe(135);\n  pricing.applyDiscount(100);",
            A::ReachedWithoutDiscriminator,
        ),
        (
            "barrel alias",
            "import { applyDiscount } from '../src/pricing';\nimport { applyDiscount as viaBarrel } from '../src';\n",
            "  expect(applyDiscount(150)).toBe(135);\n  viaBarrel(100);",
            A::Unresolved,
        ),
        (
            "unplaced receiver",
            "import { applyDiscount } from '../src/pricing';\n",
            "  const lib = loadPricing();\n  expect(applyDiscount(150)).toBe(135);\n  lib.applyDiscount(100);",
            A::Unresolved,
        ),
    ];
    for (name, imports, body, activation) in cases {
        let source = one_test(name, imports, body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, activation, "{name}");
    }
    Ok(())
}

/// Review (#5527): a describe-level binding of a constant name shadows the
/// imported constant, so the miss check never substitutes the module's
/// value (50, off the boundary) for the describe-level 100 the test passes.
#[test]
fn constant_rebound_in_an_enclosing_scope_is_unresolved() -> Result<(), String> {
    let source = concat!(
        "import { applyDiscount, SMALL } from '../src/pricing';\n",
        "describe('discounts', () => {\n",
        "  const SMALL = 100;\n",
        "  test('rebound constant', () => {\n",
        "    expect(applyDiscount(SMALL)).toBe(90);\n",
        "  });\n",
        "});\n",
    );
    let evaluated = evaluate(&DISCOUNT, source, true)?;
    let (_, relation, activation) = evaluated.row("discounts rebound constant")?;
    assert_eq!(*relation, R::DirectOwnerCall);
    assert_eq!(*activation, A::Unresolved);
    Ok(())
}

/// Review (#5527): only a closed body can establish a miss. An unobserved
/// boundary call in a hook, a helper that asserts, a hand-written check, a
/// nested block, an unrecognized matcher, or a test with no assertion all
/// leave inputs or checks the row cannot see.
#[test]
fn open_test_body_or_file_is_unresolved_not_missed() -> Result<(), String> {
    let observed = "  expect(applyDiscount(150)).toBe(135);";
    let cases = [
        (
            "unobserved hook call",
            format!("{IMPORT_DISCOUNT}\nbeforeEach(() => {{\n  applyDiscount(100);\n}});\n"),
            observed.to_string(),
        ),
        (
            "asserting helper",
            format!(
                "{IMPORT_DISCOUNT}\nfunction expectDiscountAt(n: number, e: number) {{\n  expect(applyDiscount(n)).toBe(e);\n}}\n"
            ),
            format!("{observed}\n  expectDiscountAt(100, 90);"),
        ),
        (
            "hand-written check",
            "import { applyDiscount, checkoutTotal } from '../src/pricing';\n".to_string(),
            format!(
                "{observed}\n  if (checkoutTotal(100) !== 90) {{\n    throw new Error('bad');\n  }}"
            ),
        ),
        (
            "nested block constant",
            format!("{IMPORT_DISCOUNT}\n"),
            format!("  {{\n    const LIMIT = 50;\n  }}\n{observed}"),
        ),
        (
            "custom matcher only",
            format!("{IMPORT_DISCOUNT}\n"),
            "  expect(applyDiscount(150)).toBeDiscounted();".to_string(),
        ),
        (
            "no assertion",
            format!("{IMPORT_DISCOUNT}\n"),
            "  applyDiscount(150);".to_string(),
        ),
    ];
    for (name, imports, body) in cases {
        let source = one_test(name, &imports, &body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{name}");
    }
    Ok(())
}

/// Review (#5527): a sibling test's text repeated outside the tests (here
/// inside a hook) cannot be blanked without also blanking that copy, so the
/// file scan refuses rather than miss the hook's boundary call.
#[test]
fn repeated_sibling_test_text_is_unresolved_not_missed() -> Result<(), String> {
    let sibling = "test('sibling', () => {\n  applyDiscount(100);\n});";
    let source = format!(
        "{IMPORT_DISCOUNT}\nbeforeAll(() => {{\n{sibling}\n}});\n{sibling}\ntest('target', () => {{\n  expect(applyDiscount(150)).toBe(135);\n}});\n"
    );
    let evaluated = evaluate(&DISCOUNT, &source, true)?;
    assert_eq!(evaluated.row("target")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("target")?, A::Unresolved);
    Ok(())
}

/// Review (#5527): an UPPER_CASE import from a module other than the
/// owner's may be a function, getter or object that reaches the owner, so
/// neither a hook call, an unread argument call nor a matcher member read
/// through it can stand beside a miss.
#[test]
fn foreign_constant_shaped_import_is_unresolved_not_missed() -> Result<(), String> {
    let import = format!("{IMPORT_DISCOUNT}\nimport {{ HELPER, PROBE }} from './helpers';");
    let cases = [
        (
            "hook",
            format!("{import}\nbeforeEach(() => {{\n  HELPER(100);\n}});"),
            "  expect(applyDiscount(150)).toBe(135);",
        ),
        (
            "argument call",
            import.clone(),
            "  expect(applyDiscount(150, HELPER(100))).toBe(135);",
        ),
        (
            "matcher getter",
            import.clone(),
            "  expect(applyDiscount(150)).toBe(PROBE.V);",
        ),
    ];
    for (name, imports, body) in cases {
        let source = one_test(name, &imports, body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{name}");
    }
    Ok(())
}

/// The same body-only shapes reach the closed rule without the foreign
/// import: an unread non-literal argument or a non-literal matcher argument
/// still refuses a miss, and a literal one keeps it.
#[test]
fn non_literal_call_or_matcher_argument_is_unresolved_not_missed() -> Result<(), String> {
    let cases = [
        (
            "unread object argument",
            "  expect(applyDiscount(150, { a: 1 })).toBe(135);",
            A::Unresolved,
        ),
        (
            "matcher identifier",
            "  expect(applyDiscount(150)).toBe(expected);",
            A::Unresolved,
        ),
        (
            "matcher template",
            "  expect(applyDiscount(150)).toBe(`${135}`);",
            A::Unresolved,
        ),
        (
            "literal matcher object",
            "  expect(applyDiscount(150)).toEqual({ total: 135, ok: true });",
            A::MissedBoundary,
        ),
    ];
    for (name, body, expected) in cases {
        let source = one_test(name, IMPORT_DISCOUNT, body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, expected, "{name}");
    }
    Ok(())
}

/// Review (#5527): a side-effect import runs a module the row cannot see,
/// and an unterminated one must not swallow the semicolon-free helper after
/// it; an `import x = require(...)` loads a module the same way.
#[test]
fn side_effect_or_require_import_is_unresolved_not_missed() -> Result<(), String> {
    let cases = [
        (
            "side effect without semicolon",
            format!(
                "{IMPORT_DISCOUNT}\nimport './setup'\nconst discounted = (n: number) => applyDiscount(n)\n"
            ),
        ),
        (
            "side effect",
            format!("{IMPORT_DISCOUNT}\nimport \"./setup\";\n"),
        ),
        (
            "import require",
            format!("{IMPORT_DISCOUNT}\nimport fs = require('fs');\n"),
        ),
    ];
    for (name, imports) in cases {
        let source = one_test(name, &imports, "  expect(applyDiscount(150)).toBe(135);");
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{name}");
    }
    Ok(())
}

/// Review (#5527): code outside the test bodies can load the owner module
/// and call the owner by a computed key without spelling its name or a
/// loader the scan knows, so any non-inert statement there refuses. Inside
/// the body, `_` is no integer and a ternary branch is no object key.
#[test]
fn computed_owner_access_outside_the_body_is_unresolved_not_missed() -> Result<(), String> {
    let observed = "  expect(applyDiscount(150)).toBe(135);";
    let getter = |name: &str| {
        format!(
            "{IMPORT_DISCOUNT}\nObject.defineProperty(globalThis, '{name}', {{ get: () => require ('../src/pricing')['apply' + 'Discount'](100) }});\n"
        )
    };
    let cases = [
        (
            "spaced require",
            format!(
                "{IMPORT_DISCOUNT}\nbeforeEach(() => {{\n  const m = require ('../src/pricing');\n  m['apply' + 'Discount'](100);\n}});\n"
            ),
            observed.to_string(),
        ),
        (
            "framework loader",
            format!(
                "{IMPORT_DISCOUNT}\nimport {{ vi }} from 'vitest';\nbeforeEach(async () => {{\n  const m: any = await vi.importActual('../src/pricing');\n  m['apply' + 'Discount'](100);\n}});\n"
            ),
            observed.to_string(),
        ),
        (
            "underscore global argument",
            getter("_"),
            "  expect(applyDiscount(150, _)).toBe(135);".to_string(),
        ),
        (
            "ternary branch matcher",
            getter("Q"),
            "  expect(applyDiscount(150)).toBe(true ? Q : 135);".to_string(),
        ),
    ];
    for (name, imports, body) in cases {
        let source = one_test(name, &imports, &body);
        let evaluated = evaluate(&DISCOUNT, &source, true)?;
        assert_eq!(evaluated.row(name)?.1, R::DirectOwnerCall, "{name}");
        assert_eq!(evaluated.activation(name)?, A::Unresolved, "{name}");
    }
    Ok(())
}

/// The inert-residue rule still admits a miss inside a `describe` wrapper
/// with comments around it.
#[test]
fn describe_wrapper_and_comments_keep_a_closed_miss() -> Result<(), String> {
    let source = format!(
        "{IMPORT_DISCOUNT}\n// pricing tests\n/* off-boundary only */\ndescribe('pricing', () => {{\n  test('wrapped', () => {{\n    expect(applyDiscount(150)).toBe(135);\n  }});\n}});\n"
    );
    let evaluated = evaluate(&DISCOUNT, &source, true)?;
    assert_eq!(evaluated.row("pricing wrapped")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("pricing wrapped")?, A::MissedBoundary);
    Ok(())
}

/// Review (#5527): an owner reached again through another function (mutual
/// recursion) can carry an off-boundary input to the boundary.
#[test]
fn mutually_recursive_owner_is_unresolved_not_missed() -> Result<(), String> {
    let source = one_test(
        "mutual recursion",
        "import { band } from '../src/pricing';",
        "  expect(band(5)).toBe(1);",
    );
    let evaluated = evaluate(&BAND, &source, true)?;
    assert_eq!(evaluated.row("mutual recursion")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("mutual recursion")?, A::Unresolved);
    Ok(())
}

/// Review (#5527): an owner that calls itself can carry an off-boundary
/// input to the boundary internally, so its rows never miss.
#[test]
fn recursive_owner_is_unresolved_not_missed() -> Result<(), String> {
    let source = one_test(
        "recursive",
        "import { tier } from '../src/pricing';",
        "  expect(tier(5)).toBe(1);",
    );
    let evaluated = evaluate(&TIER, &source, true)?;
    assert_eq!(evaluated.row("recursive")?.1, R::DirectOwnerCall);
    assert_eq!(evaluated.activation("recursive")?, A::Unresolved);
    Ok(())
}

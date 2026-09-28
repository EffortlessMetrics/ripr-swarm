//! Tests for the TypeScript boundary input derivation (#4215 follow-up).

use super::*;
use std::path::Path;

const CHANGED_LINE: &str = "  if (amount >= DISCOUNT_THRESHOLD) {";

/// The boundary input for `changed_line` inside `owner_name`, extracted from
/// `source` exactly as the adapter extracts it. The outer `None` means the
/// owner was not extracted (for example a parse failure), so a negative case
/// asserts `Some(None)` and can never pass on a broken source alone.
fn input_for(
    source: &str,
    owner_name: &str,
    changed_line: &str,
) -> Option<Option<TypeScriptBoundaryInput>> {
    let owners = extract_owners(Path::new("src/pricing.ts"), source);
    let owner = owners.iter().find(|owner| owner.name == owner_name)?;
    Some(ts_boundary_input_in_source(source, changed_line, owner))
}

fn pricing_module(prelude: &str, body_extra: &str, postlude: &str) -> String {
    format!(
        "{prelude}\nexport function discountedTotal(amount: number): number {{\n{body_extra}\n  if (amount >= DISCOUNT_THRESHOLD) {{\n    return amount - Math.floor(amount / 10);\n  }}\n  return amount;\n}}\n{postlude}\n"
    )
}

fn resolved(value: i64) -> Option<TypeScriptBoundaryInput> {
    Some(TypeScriptBoundaryInput {
        parameter: "amount".to_string(),
        index: 0,
        operand: "DISCOUNT_THRESHOLD".to_string(),
        value,
    })
}

#[test]
fn same_file_integer_const_resolves_to_the_boundary_input() {
    let source = pricing_module("export const DISCOUNT_THRESHOLD = 10000;", "", "");
    // Setup: the owner parsed with its single plain parameter.
    let owners = extract_owners(Path::new("src/pricing.ts"), &source);
    let owner = owners.iter().find(|owner| owner.name == "discountedTotal");
    assert_eq!(
        owner.map(|owner| owner.params.clone()),
        Some(vec!["amount".to_string()])
    );

    let input = input_for(&source, "discountedTotal", CHANGED_LINE).flatten();
    assert_eq!(input, resolved(10000));
    assert_eq!(
        input.map(|input| input.evidence_line()),
        Some(
            "typescript_boundary_input: parameter=amount;index=0;operand=DISCOUNT_THRESHOLD;value=10000"
                .to_string()
        )
    );
}

#[test]
fn benign_module_shapes_still_resolve() {
    for (label, prelude, body_extra, postlude) in [
        (
            "underscore literal",
            "const DISCOUNT_THRESHOLD: number = 10_000;",
            "",
            "",
        ),
        (
            "comments and strings name both",
            "// DISCOUNT_THRESHOLD = 5 and amount = 1 in a comment\nexport const DISCOUNT_THRESHOLD = 10000;",
            "  /* let amount = 2; */ const label = \"amount = DISCOUNT_THRESHOLD\";",
            "",
        ),
        (
            "reads in calls, templates, typeof, and re-exports",
            "const DISCOUNT_THRESHOLD = 10000;\nexport { DISCOUNT_THRESHOLD };\nexport const cap = Math.max(1, DISCOUNT_THRESHOLD);",
            "  log(`${amount} vs ${DISCOUNT_THRESHOLD}`, typeof DISCOUNT_THRESHOLD, format(amount));",
            "export default DISCOUNT_THRESHOLD;",
        ),
        (
            "a member of the same name is a different property",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const limits = order.amount + config.DISCOUNT_THRESHOLD;",
            "",
        ),
    ] {
        let source = pricing_module(prelude, body_extra, postlude);
        assert_eq!(
            input_for(&source, "discountedTotal", CHANGED_LINE).flatten(),
            resolved(10000),
            "{label}: {source}"
        );
    }
}

#[test]
fn constant_written_first_resolves_with_the_parameter_side() {
    let source = "export const DISCOUNT_THRESHOLD = 10000;\nexport function discountedTotal(amount: number): number {\n  if (DISCOUNT_THRESHOLD <= amount) {\n    return 1;\n  }\n  return amount;\n}\n";
    assert_eq!(
        input_for(
            source,
            "discountedTotal",
            "  if (DISCOUNT_THRESHOLD <= amount) {"
        )
        .flatten(),
        resolved(10000)
    );
}

#[test]
fn literal_boundary_binds_the_parameter_position() {
    let source = "export function applyRate(rate: number, amount: number): number {\n  if (amount >= 100) {\n    return rate;\n  }\n  return 0;\n}\n";
    assert_eq!(
        input_for(source, "applyRate", "  if (amount >= 100) {").flatten(),
        Some(TypeScriptBoundaryInput {
            parameter: "amount".to_string(),
            index: 1,
            operand: "100".to_string(),
            value: 100,
        })
    );
}

#[test]
fn rebindable_or_non_literal_constants_stay_unresolved() {
    for (label, prelude, body_extra, postlude) in [
        ("let", "export let DISCOUNT_THRESHOLD = 10000;", "", ""),
        ("var", "export var DISCOUNT_THRESHOLD = 10000;", "", ""),
        (
            "float",
            "export const DISCOUNT_THRESHOLD = 10000.5;",
            "",
            "",
        ),
        ("exponent", "export const DISCOUNT_THRESHOLD = 1e4;", "", ""),
        ("hex", "export const DISCOUNT_THRESHOLD = 0x2710;", "", ""),
        (
            "bigint",
            "export const DISCOUNT_THRESHOLD = 10000n;",
            "",
            "",
        ),
        (
            "computed",
            "export const DISCOUNT_THRESHOLD = limit();",
            "",
            "",
        ),
        (
            "as const",
            "export const DISCOUNT_THRESHOLD = 10000 as const;",
            "",
            "",
        ),
        ("negative", "export const DISCOUNT_THRESHOLD = -1;", "", ""),
        (
            "declare only",
            "export declare const DISCOUNT_THRESHOLD: number;",
            "",
            "",
        ),
        ("no declaration", "", "", ""),
        (
            "imported from another module",
            "import { DISCOUNT_THRESHOLD } from './config';",
            "",
            "",
        ),
        (
            "imported under an alias",
            "import { LIMIT as DISCOUNT_THRESHOLD } from './config';",
            "",
            "",
        ),
        (
            "declared twice at the top level",
            "const DISCOUNT_THRESHOLD = 10000;\nconst DISCOUNT_THRESHOLD = 5;",
            "",
            "",
        ),
        (
            "destructured at the top level",
            "export const DISCOUNT_THRESHOLD = 10000;\nconst [DISCOUNT_THRESHOLD] = [5];",
            "",
            "",
        ),
        (
            "shadowed by a local const in the owner",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const DISCOUNT_THRESHOLD = 5;",
            "",
        ),
        (
            "shadowed by a nested block let",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  { let DISCOUNT_THRESHOLD = 5; }",
            "",
        ),
        (
            "shadowed by a nested arrow parameter",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const check = (DISCOUNT_THRESHOLD: number) => amount;",
            "",
        ),
        (
            "shadowed by a bare arrow parameter",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const check = DISCOUNT_THRESHOLD => amount;",
            "",
        ),
        (
            "shadowed by a catch binding",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  try { run(); } catch (DISCOUNT_THRESHOLD) { log(); }",
            "",
        ),
        (
            "shadowed by a for-of binding",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  for (const DISCOUNT_THRESHOLD of [1]) { log(); }",
            "",
        ),
        (
            "shadowed by object destructuring",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const { DISCOUNT_THRESHOLD } = settings;",
            "",
        ),
        (
            "shadowed by renamed destructuring",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const { limit: DISCOUNT_THRESHOLD } = settings;",
            "",
        ),
        (
            "nested function of the same name",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  function DISCOUNT_THRESHOLD() { return 5; }",
            "",
        ),
        (
            "nested generator of the same name",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  function* DISCOUNT_THRESHOLD() { yield 5; }",
            "",
        ),
        (
            "enum member of the same name",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "",
            "enum Limits { DISCOUNT_THRESHOLD = 5 }",
        ),
        (
            "namespace const of the same name",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "",
            "namespace Limits { export const DISCOUNT_THRESHOLD = 5; }",
        ),
        (
            "class of the same name elsewhere",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "",
            "class DISCOUNT_THRESHOLD {}",
        ),
        (
            "object key write of the same name",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "",
            "exports.DISCOUNT_THRESHOLD = 5;",
        ),
        (
            "object literal key of the same name",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "",
            "export const settings = { DISCOUNT_THRESHOLD: 5 };",
        ),
        (
            "eval in the module",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  eval(code);",
            "",
        ),
        (
            "escaped identifier",
            "export const DISCOUNT_THRESHOLD = 10000;",
            "  const \\u0044ISCOUNT_THRESHOLD = 5;",
            "",
        ),
    ] {
        let source = pricing_module(prelude, body_extra, postlude);
        assert_eq!(
            input_for(&source, "discountedTotal", CHANGED_LINE),
            Some(None),
            "{label}: {source}"
        );
    }
}

#[test]
fn owner_parameter_shadow_of_the_constant_stays_unresolved() {
    let source = "export const DISCOUNT_THRESHOLD = 10000;\nexport function discountedTotal(amount: number, DISCOUNT_THRESHOLD = 5): number {\n  if (amount >= DISCOUNT_THRESHOLD) {\n    return 1;\n  }\n  return amount;\n}\n";
    assert_eq!(
        input_for(source, "discountedTotal", CHANGED_LINE),
        Some(None)
    );
}

#[test]
fn written_or_rebound_parameters_stay_unresolved() {
    for (label, body_extra) in [
        ("assignment", "  amount = Math.round(amount);"),
        ("compound assignment", "  amount += 1;"),
        ("postfix update", "  amount++;"),
        ("prefix update", "  --amount;"),
        ("var redeclaration", "  var amount = 2;"),
        ("nested block let", "  { let amount = 2; log(amount); }"),
        ("array destructuring write", "  [amount] = [1];"),
        ("object destructuring write", "  ({ amount } = settings);"),
        ("for-of write", "  for (amount of [1]) { log(); }"),
        ("for-in write", "  for (amount in settings) { log(); }"),
        (
            "nested closure parameter",
            "  const f = (amount: number) => amount;",
        ),
        (
            "nested function parameter",
            "  function inner(amount: number) { return amount; }",
        ),
        ("arguments alias", "  arguments[0] = 5;"),
    ] {
        let source = pricing_module("export const DISCOUNT_THRESHOLD = 10000;", body_extra, "");
        assert_eq!(
            input_for(&source, "discountedTotal", CHANGED_LINE),
            Some(None),
            "{label}: {source}"
        );
    }
}

#[test]
fn comparisons_without_a_single_plain_parameter_stay_unresolved() {
    let module = "export const LIMIT = 10;\n";
    for (label, signature, line) in [
        (
            "both sides are parameters",
            "(amount: number, limit: number)",
            "  if (amount >= limit) {",
        ),
        (
            "member receiver",
            "(order: Order)",
            "  if (order.amount >= LIMIT) {",
        ),
        (
            "length receiver",
            "(user: string)",
            "  if (user.length >= LIMIT) {",
        ),
        (
            "destructured parameter",
            "({ amount }: Order)",
            "  if (amount >= LIMIT) {",
        ),
        (
            "rest parameter",
            "(...amount: number[])",
            "  if (amount >= LIMIT) {",
        ),
        (
            "nullish boundary",
            "(amount?: number)",
            "  if (amount ?? 10) {",
        ),
        (
            "float literal",
            "(amount: number)",
            "  if (amount >= 10.5) {",
        ),
    ] {
        let source = format!(
            "{module}export function check{signature}: number {{\n{line}\n    return 1;\n  }}\n  return 0;\n}}\n"
        );
        assert_eq!(
            input_for(&source, "check", line),
            Some(None),
            "{label}: {source}"
        );
    }
}

#[test]
fn unparseable_module_stays_unresolved() {
    // The owner is extracted from a parseable copy; the module read at
    // derivation time does not parse.
    let good = pricing_module("export const DISCOUNT_THRESHOLD = 10000;", "", "");
    let owners = extract_owners(Path::new("src/pricing.ts"), &good);
    let owner = owners.iter().find(|owner| owner.name == "discountedTotal");
    assert!(owner.is_some(), "setup: owner must be extracted");
    let broken = pricing_module("export const DISCOUNT_THRESHOLD = 10000;", "", "const = ;");
    assert_eq!(
        owner.and_then(|owner| ts_boundary_input_in_source(&broken, CHANGED_LINE, owner)),
        None
    );
    assert_eq!(
        owner.and_then(|owner| ts_boundary_input_in_source(&good, CHANGED_LINE, owner)),
        resolved(10000)
    );
}

/// Review finding on #4429: operand extraction keeps only the token nearest
/// the comparison, so arithmetic, a sign, or a member read around either side
/// used to derive the operand's value as the boundary input. Each shape moves
/// the real boundary, so no input is derived.
#[test]
fn comparisons_with_arithmetic_or_signed_sides_stay_unresolved() {
    let source = pricing_module(
        "export const DISCOUNT_THRESHOLD = 10000;\nexport const OFFSET = 5;",
        "",
        "",
    );
    // Control: the plain comparison still resolves, so the negatives below
    // fail on the changed line, not on the module.
    assert_eq!(
        input_for(&source, "discountedTotal", CHANGED_LINE),
        Some(resolved(10000))
    );
    for changed in [
        "  if (OFFSET + amount >= DISCOUNT_THRESHOLD) {",
        "  if (amount >= DISCOUNT_THRESHOLD + 1) {",
        "  if (amount * 2 > DISCOUNT_THRESHOLD) {",
        "  if (2 * amount >= DISCOUNT_THRESHOLD) {",
        "  if (amount - OFFSET >= DISCOUNT_THRESHOLD) {",
        "  if (DISCOUNT_THRESHOLD <= amount < OFFSET) {",
        "  if (amount >= -5) {",
        "  if (order.amount >= DISCOUNT_THRESHOLD) {",
    ] {
        assert_eq!(
            input_for(&source, "discountedTotal", changed),
            Some(None),
            "{changed}"
        );
    }
}

#[test]
fn whole_side_comparisons_accept_common_line_shapes() {
    for (line, expected) in [
        ("  if (amount >= LIMIT) {", true),
        ("  return amount >= LIMIT ? 1 : 0;", true),
        ("  const big = LIMIT <= amount && flag;", true),
        ("if (ok || amount === 5_000) {", true),
        ("  if (amount >= LIMIT + 1) {", false),
        ("  if (x + amount >= LIMIT) {", false),
        ("  if (amount >= -LIMIT) {", false),
        ("  if (amount >= LIMIT || amount >= LIMIT) {", false),
    ] {
        let operand = if line.contains("5_000") {
            "5_000"
        } else {
            "LIMIT"
        };
        assert_eq!(
            super::boundary_input::comparison_has_whole_sides(line, "amount", operand),
            expected,
            "{line}"
        );
    }
}

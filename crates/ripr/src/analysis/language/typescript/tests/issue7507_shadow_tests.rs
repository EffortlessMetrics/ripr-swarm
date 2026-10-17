//! Issue #7507: owner-name shadow checks miss catch, parameter, for-of and
//! multi-declarator bindings, plus member-call vs reference, `typeof`,
//! default-export spy fabrication, test-file re-export imports, dead owners
//! rebound before `export default`, and conflicting bare `module.exports`
//! assignments — each reading false `exposed`.
//!
//! Every case asserts the parsed subject (owner facts, import record,
//! relation, assertion) before the class, so a fixture that stops parsing
//! the intended shape fails here rather than passing for the wrong reason.

use super::*;

const LIB_FILE: &str = "src/lib.ts";
const TEST_FILE: &str = "tests/lib.test.ts";
const CHANGED_LINE: usize = 3;
const CHANGED_TEXT: &str = "    return amount - 20;";

const PRICE_BODY: &str = "(amount: number): number {\n  if (amount > 100) {\n    return amount - 20;\n  }\n  return amount;\n}\n";

fn named_lib() -> String {
    format!("export function price{PRICE_BODY}")
}

fn default_lib() -> String {
    format!("export default function price{PRICE_BODY}")
}

fn owners_of_lib(source: &str) -> Vec<TypeScriptOwner> {
    extract_owners(Path::new(LIB_FILE), source)
}

fn tests_of(source: &str) -> Vec<TypeScriptTest> {
    extract_tests(Path::new(TEST_FILE), source)
}

fn classify_at_changed(
    owners: &[TypeScriptOwner],
    tests: &[TypeScriptTest],
) -> Result<Finding, String> {
    classify_change(
        Path::new(LIB_FILE),
        CHANGED_LINE,
        CHANGED_TEXT,
        owners,
        tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "expected a finding".to_string())
}

fn relations_of(owner: &TypeScriptOwner, tests: &[TypeScriptTest]) -> Vec<TypeScriptRelationKind> {
    related_test_candidates(owner, tests, None, &ReExportIndex::empty(), None)
        .into_iter()
        .map(|candidate| candidate.relation)
        .collect()
}

fn price_owner_named(owners: &[TypeScriptOwner]) -> Result<&TypeScriptOwner, String> {
    owners
        .iter()
        .find(|owner| owner.name == "price")
        .ok_or_else(|| format!("no `price` owner in {owners:?}"))
}

fn assert_single_strong_exact(
    tests: &[TypeScriptTest],
    expected_observed: &str,
) -> Result<(), String> {
    let [test] = tests else {
        return Err(format!("expected one test, got {}", tests.len()));
    };
    let strong: Vec<&TypeScriptAssertion> = test
        .assertions
        .iter()
        .filter(|assertion| {
            assertion.oracle_kind == OracleKind::ExactValue
                && assertion.oracle_strength == OracleStrength::Strong
        })
        .collect();
    let [assertion] = strong.as_slice() else {
        return Err(format!("expected one strong exact assertion: {test:?}"));
    };
    let observed = assertion
        .observed_expression
        .as_deref()
        .ok_or_else(|| format!("expected observed_expression: {assertion:?}"))?;
    if observed != expected_observed {
        return Err(format!(
            "observed `{observed}`, expected `{expected_observed}`"
        ));
    }
    Ok(())
}

#[test]
fn issue7507_catch_param_shadows_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  try { throw ((a: number) => 140); } catch (price) { expect(price(150)).toBe(140); }\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "catch parameter must shadow the owner call: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("catch binding must read as a body-local shadow".to_string());
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("catch-param helper must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_catch_param_shadows_alias_local() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price as cost } from '../src/lib';\ntest('p', () => {\n  try { throw ((a: number) => 140); } catch (cost) { expect(cost(150)).toBe(140); }\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "cost(150)")?;
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "catch parameter must shadow the alias call: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("catch-param alias helper must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_arrow_param_shadows_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect(((price: any) => price(150))((a: number) => 140)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "((price: any) => price(150))((a: number) => 140)")?;
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("arrow parameter binding must read as a body-local shadow".to_string());
    }
    // The bare `price(150);` smoke statement keeps a direct-call reading only
    // when the observed assertion itself reaches the owner; the observed
    // expression here calls the parameter, so the finding must not be
    // exposed via this test alone. The relation gate conservatively withholds
    // the whole test once any binding shadows the name.
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "arrow-param shadow must withhold the trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("arrow-param helper must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_for_of_binding_shadows_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  for (const price of [(a: number) => 140]) { expect(price(150)).toBe(140); }\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("for-of binding must read as a body-local shadow".to_string());
    }
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "for-of binding must withhold the trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("for-of helper must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_multi_declarator_shadows_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  const x = 1, price = (a: number) => 140; expect(price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("non-first declarator must read as a body-local shadow".to_string());
    }
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "multi-declarator shadow must withhold the trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("multi-declarator helper must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_member_call_is_not_an_owner_reference() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect(helper.price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "helper.price(150)")?;
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("member call on another receiver must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_typeof_observation_is_non_value() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect(typeof price).toBe(\"function\");\n});\n";
    let tests = tests_of(source);
    if tests.is_empty() || tests[0].assertions.is_empty() {
        return Err("expected the typeof assertion to be extracted".to_string());
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("typeof observation must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_spy_on_default_fabricates_default_export() -> Result<(), String> {
    let owners = owners_of_lib(&default_lib());
    let owner = price_owner_named(&owners)?.clone();
    if !owner.exported_as_default {
        return Err("default-export owner must carry exported_as_default".to_string());
    }
    let source = "import price from '../src/lib';\nimport * as mod from '../src/lib';\ntest('p', () => {\n  jest.spyOn(mod, 'default').mockReturnValue(140); expect(price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if !test_spies_owner_with_fabrication_for_owner(&tests[0], "price", owner.exported_as_default) {
        return Err("spyOn(mod, 'default') must fabricate the default export".to_string());
    }
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "fabricated default spy must withhold the trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("spy-fabricated default observer must not read exposed".to_string());
    }
    Ok(())
}

#[test]
fn issue7507_test_file_reexport_is_not_an_import() -> Result<(), String> {
    let owners = owners_of_lib(&default_lib());
    let owner = price_owner_named(&owners)?.clone();
    if !owner.exported_as_default {
        return Err("default-export owner must carry exported_as_default".to_string());
    }
    let source = "export { default as cost } from '../src/lib';\nconst cost = (a: number) => 140;\ntest('p', () => {\n  expect(cost(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    if tests.is_empty() {
        return Err("expected the test to be extracted".to_string());
    }
    if tests[0]
        .imports_in_file
        .iter()
        .any(|import| import.local == "cost")
    {
        return Err(format!(
            "test-file re-export must not be recorded as an import: {:?}",
            tests[0].imports_in_file
        ));
    }
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "re-exported helper must not gain a trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    Ok(())
}

#[test]
fn issue7507_owner_rebound_before_default_export_is_dead() -> Result<(), String> {
    let source = "let price = (amount: number) => amount - 10;\nprice = (amount: number) => 140;\nexport default price;\n";
    let owners = owners_of_lib(source);
    if owners.iter().any(|owner| owner.name == "price") {
        return Err(format!(
            "reassigned owner binding is dead and must yield no owner: {owners:?}"
        ));
    }
    Ok(())
}

#[test]
fn issue7507_conflicting_bare_module_exports_yield_no_owner() -> Result<(), String> {
    let source = "module.exports = function alpha(amount) { return amount - 20; };\nmodule.exports = function beta(amount) { return amount - 20; };\n";
    let owners = extract_owners(Path::new("src/lib.js"), source);
    if owners.iter().any(|owner| owner.exported_as_default) {
        return Err(format!(
            "conflicting bare module.exports assignments must yield no default owner: {owners:?}"
        ));
    }
    Ok(())
}

/// Preservation: a genuine `ns.price(150)` through an owner-module
/// namespace receiver still observes the owner and still reads `exposed`.
/// Guards the new `ts_observed_namespaced_owner_call` path against
/// over-withholding regressions.
#[test]
fn issue7507_genuine_namespace_call_stays_exposed() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import * as ns from '../src/lib';\ntest('p', () => {\n  expect(ns.price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "ns.price(150)")?;
    if !relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "genuine namespace call must keep a trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "genuine namespace call must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// A body-local `ns` shadows the namespace import, so `ns.price(...)` is
/// not an owner call. A real `price(...)` smoke call keeps the test
/// related; the pin must not confirm `exposed`.
#[test]
fn issue7507_shadowed_namespace_call_is_not_exposed() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import * as ns from '../src/lib';\nimport { price } from '../src/lib';\ntest('p', () => {\n  const ns = fakeLib();\n  price(150);\n  expect(ns.price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "ns.price(150)")?;
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "ns") {
        return Err("body-local `ns` must read as a shadow".to_string());
    }
    if !relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "real owner smoke call must keep a trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("shadowed-namespace pin must not read exposed".to_string());
    }
    Ok(())
}

/// A test-local `function price(a) { ... }` declaration shadows the owner
/// even though its declarator carries a body brace.
#[test]
fn issue7507_function_declaration_shadows_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  function price(a: number) { return 140; }\n  expect(price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("function declaration must read as a body-local shadow".to_string());
    }
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "function declaration must shadow the owner call: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("function-declared helper must not read exposed".to_string());
    }
    Ok(())
}

/// `other.ns.price(150)` is a different receiver's member, not the
/// owner-module namespace call, so it never confirms the owner.
#[test]
fn issue7507_nested_receiver_does_not_confirm_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import * as ns from '../src/lib';\ntest('p', () => {\n  expect(other.ns.price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "other.ns.price(150)")?;
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "nested receiver must not confirm the owner: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("nested-receiver call must not read exposed".to_string());
    }
    Ok(())
}

/// `export let price = ...` binds like `let price = ...`: a later plain
/// reassignment kills the owner instead of leaving it credited.
#[test]
fn issue7507_export_let_reassignment_drops_owner() -> Result<(), String> {
    let source = "export let price = (n: number) => n - 20;\nprice = (n: number) => 140;\n";
    let owners = owners_of_lib(source);
    if owners.iter().any(|owner| owner.name == "price") {
        return Err(format!(
            "reassigned export-let binding is dead and must yield no owner: {owners:?}"
        ));
    }
    Ok(())
}

/// A parenthesized top-level reassignment still reassigns: the dead owner
/// is dropped instead of staying credited.
#[test]
fn issue7507_parenthesized_reassignment_drops_owner() -> Result<(), String> {
    let source = "export let price = (n: number) => n - 20;\n(price = (n: number) => 140);\n";
    let owners = owners_of_lib(source);
    if owners.iter().any(|owner| owner.name == "price") {
        return Err(format!(
            "parenthesized reassignment is dead and must yield no owner: {owners:?}"
        ));
    }
    Ok(())
}

/// Reassigning a top-level `price` binding must not drop a live class
/// method that merely shares the name.
#[test]
fn issue7507_reassignment_keeps_live_class_method() -> Result<(), String> {
    let source =
        "let price = () => 1;\nexport class Cart { price() { return 2; } }\nprice = () => 3;\n";
    let owners = owners_of_lib(source);
    if !owners
        .iter()
        .any(|owner| owner.name == "price" && owner.class_name.as_deref() == Some("Cart"))
    {
        return Err(format!(
            "live class method must survive an unrelated top-level reassignment: {owners:?}"
        ));
    }
    Ok(())
}

/// A `void`-wrapped or binary top-level reassignment still reassigns: the
/// dead owner is dropped instead of staying credited.
#[test]
fn issue7507_wrapped_reassignment_drops_owner() -> Result<(), String> {
    for source in [
        "export let price = (n: number) => n - 20;\nvoid (price = (n: number) => 140);\n",
        "export let price = (n: number) => n - 20;\n0 + (price = (n: number) => 140);\n",
        "export let price = (n: number) => n - 20;\nawait (price = (n: number) => 140);\n",
    ] {
        let owners = owners_of_lib(source);
        if owners.iter().any(|owner| owner.name == "price") {
            return Err(format!(
                "wrapped reassignment is dead and must yield no owner: {owners:?}"
            ));
        }
    }
    Ok(())
}

/// A destructured object parameter binds the name exactly like a plain
/// parameter: the helper assertion must not read exposed.
#[test]
fn issue7507_destructured_param_shadows_owner() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect((({ price }) => price(150))({ price: (a: number) => 140 })).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(
        &tests,
        "(({ price }) => price(150))({ price: (a: number) => 140 })",
    )?;
    if !local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("destructured parameter binding must read as a body-local shadow".to_string());
    }
    if relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "destructured-param shadow must withhold the trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("destructured-param helper must not read exposed".to_string());
    }
    Ok(())
}

/// A parameter default initializer reads `price` but does not rebind it, so
/// the genuine owner assertion keeps its trusted credit.
#[test]
fn issue7507_param_default_use_is_not_a_shadow() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  const make = ((f = price) => f)();\n  expect(price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("a default-initializer mention must not read as a shadow".to_string());
    }
    if !relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "default use must not shadow the genuine owner relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "default use must keep the owner exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// A return-type annotation mentioning the owner (`: typeof price`) is a
/// type-space use, not a binding: the genuine owner assertion stays trusted.
#[test]
fn issue7507_return_type_use_is_not_a_shadow() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  const make = ((x): typeof price => x)(0);\n  expect(price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if local_identifier_declared_in_test_body(&tests[0].body_text, "price") {
        return Err("a return-type mention must not read as a shadow".to_string());
    }
    if !relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "return-type mention must not shadow the genuine owner relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "return-type mention must keep the owner exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// A keyword before the namespace receiver (`await ns.price(150)`) leaves
/// a genuine owner-module call: only a member dot disqualifies, never a
/// keyword's trailing letter.
#[test]
fn issue7507_keyword_qualified_namespace_call_stays_exposed() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import * as ns from '../src/lib';\ntest('p', async () => {\n  expect(await ns.price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "await ns.price(150)")?;
    if !relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "keyword-qualified namespace call must keep a trusted relation: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "keyword-qualified namespace call must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// Whitespace after an unrelated member dot (`other. ns.price(150)`) reads
/// `other.ns`'s member: the imported namespace receiver cannot be
/// established from that spelling, so the assertion must not confirm
/// exposed even with a genuine smoke call present.
#[test]
fn issue7507_whitespace_member_call_is_not_an_owner_reference() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let source = "import * as ns from '../src/lib';\ntest('p', () => {\n  const other = { ns };\n  price(150); expect(other. ns.price(150)).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "other. ns.price(150)")?;
    let finding = classify_at_changed(&owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err(
            "member call with a space after an unrelated dot must not read exposed".to_string(),
        );
    }
    Ok(())
}

/// A tiny test body exercises the dynamic-import receiver scan without
/// tripping the keyword search: a needle longer than the body matches
/// nowhere and must not panic.
#[test]
fn issue7507_dynamic_import_receivers_survive_a_tiny_body() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let tests = tests_of("test('t', () => {});\n");
    let [test] = tests.as_slice() else {
        return Err(format!("expected one test, got {}", tests.len()));
    };
    let receivers = owner_namespace_receivers(test, &owner, None, None);
    if !receivers.is_empty() {
        return Err(format!(
            "a tiny body binds no namespace receiver: {receivers:?}"
        ));
    }
    Ok(())
}

/// A call in the C-style loop condition (`i < price(2)`) is a use, not a
/// binding: the imported owner's assertions keep their trusted credit.
#[test]
fn issue7507_loop_condition_call_is_not_a_shadow() -> Result<(), String> {
    let owners = owners_of_lib(&named_lib());
    let owner = price_owner_named(&owners)?.clone();
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  for (let i = 0; i < price(2); i++) { expect(price(150)).toBe(140); }\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "price(150)")?;
    if !relations_of(&owner, &tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "loop-condition call must not shadow the owner: {:?}",
            relations_of(&owner, &tests)
        ));
    }
    let finding = classify_at_changed(&owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "loop-condition use must keep the owner exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

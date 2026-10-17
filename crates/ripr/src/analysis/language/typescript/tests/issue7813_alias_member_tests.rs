//! Issue #7813: the one-hop alias initializer must not credit a member
//! segment as an observation of a bare-call owner.
//!
//! `ts_observed_local_aliases_owner` used [`ts_references_identifier`], which
//! counts `helper.price(` as a reference to `price`. For `export function
//! price`, that member is a different function, so `const r = helper.price(150);
//! expect(r).toBe(140)` could mark a smoke call `exposed`. Method owners keep
//! today's member-segment credit: their honest needle is `receiver.method(...)`.
//!
//! Declared from `mod.rs` rather than `tests.rs` so this claim does not
//! collide with #8476, which edits `tests.rs`.

use super::*;
use std::path::Path;

const LIB_FILE: &str = "src/lib.ts";
const TEST_FILE: &str = "tests/lib.test.ts";
const CHANGED_TEXT: &str = "    return amount - 20;";

const PRICE_BODY: &str = "(amount: number): number {\n  if (amount > 100) {\n    return amount - 20;\n  }\n  return amount;\n}\n";

fn named_lib() -> String {
    format!("export function price{PRICE_BODY}")
}

fn class_instance_lib() -> String {
    format!("export class Cart {{\n  price{PRICE_BODY}}}\n")
}

fn class_static_lib() -> String {
    format!("export class Cart {{\n  static price{PRICE_BODY}}}\n")
}

fn owners_of_lib(source: &str) -> Vec<TypeScriptOwner> {
    extract_owners(Path::new(LIB_FILE), source)
}

fn tests_of(source: &str) -> Vec<TypeScriptTest> {
    extract_tests(Path::new(TEST_FILE), source)
}

fn price_owner_named(owners: &[TypeScriptOwner]) -> Result<&TypeScriptOwner, String> {
    owners
        .iter()
        .find(|owner| owner.name == "price")
        .ok_or_else(|| format!("no `price` owner in {owners:?}"))
}

fn changed_line_of(source: &str) -> Result<usize, String> {
    source
        .lines()
        .position(|line| line.contains("return amount - 20"))
        .map(|index| index + 1)
        .ok_or_else(|| "lib is missing the discount return".to_string())
}

fn classify_at_changed(
    source: &str,
    owners: &[TypeScriptOwner],
    tests: &[TypeScriptTest],
) -> Result<Finding, String> {
    classify_change(
        Path::new(LIB_FILE),
        changed_line_of(source)?,
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

fn assert_trusted_relation(
    owner: &TypeScriptOwner,
    tests: &[TypeScriptTest],
) -> Result<(), String> {
    if !relations_of(owner, tests)
        .iter()
        .any(|relation| relation.uses_oracle())
    {
        return Err(format!(
            "expected a trusted relation, got {:?}",
            relations_of(owner, tests)
        ));
    }
    Ok(())
}

/// The issue repro: a smoke `price(150)` keeps the test related, but the
/// asserted local is initialized from `helper.price(150)` — a different
/// function — so the finding must not read `exposed`.
#[test]
fn issue7813_member_initializer_does_not_expose_bare_call_owner() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    if !matches!(owner.owner_kind, OwnerKind::Function) {
        return Err(format!(
            "bare-call owner must be a function, got {:?}",
            owner.owner_kind
        ));
    }
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150);\n  const r = helper.price(150);\n  expect(r).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("member-initializer of a bare-call owner must not read exposed".to_string());
    }
    Ok(())
}

/// Optional chaining is still a member segment, not a bare owner call.
#[test]
fn issue7813_optional_member_initializer_does_not_expose_bare_call_owner() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150);\n  const r = helper?.price(150);\n  expect(r).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("optional member initializer must not read exposed".to_string());
    }
    Ok(())
}

/// Non-null assertion member access is still a member segment.
#[test]
fn issue7813_non_null_member_initializer_does_not_expose_bare_call_owner() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150);\n  const r = helper!.price(150);\n  expect(r).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("non-null member initializer must not read exposed".to_string());
    }
    Ok(())
}

/// Whitespace between the dot and the member name is still a member segment
/// (`helper. price(`), matching the #7507 direct-observation rule.
#[test]
fn issue7813_whitespace_member_initializer_does_not_expose_bare_call_owner() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150);\n  const r = helper. price(150);\n  expect(r).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("whitespace member initializer must not read exposed".to_string());
    }
    Ok(())
}

/// An alias-rename local is still not observed through another receiver's
/// same-named member (`helper.cost` is not `import { price as cost }`).
#[test]
fn issue7813_member_initializer_does_not_expose_alias_rename() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price as cost } from '../src/lib';\ntest('p', () => {\n  cost(150);\n  const r = helper.cost(150);\n  expect(r).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err(
            "member initializer of an alias-rename local must not read exposed".to_string(),
        );
    }
    Ok(())
}

/// Preservation: capturing a genuine bare owner call in a local still
/// observes the changed sink.
#[test]
fn issue7813_bare_call_alias_initializer_stays_exposed() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  const r = price(150);\n  expect(r).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "bare owner-call alias must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// Preservation: a namespace import of the owner's module still observes
/// through `const r = ns.price(150)`, matching the #7507 namespaced-owner
/// exception on the direct value guard.
#[test]
fn issue7813_namespace_alias_initializer_stays_exposed() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import * as ns from '../src/lib';\ntest('p', () => {\n  const r = ns.price(150);\n  expect(r).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "namespace owner-call alias must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// Method owners keep today's member-segment credit: `obj.price` is the
/// honest needle, so asserting the local initialized from it stays `exposed`.
#[test]
fn issue7813_instance_method_alias_initializer_stays_exposed() -> Result<(), String> {
    let lib = class_instance_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    if !matches!(owner.owner_kind, OwnerKind::Method) {
        return Err(format!(
            "instance method owner must be Method, got {:?}",
            owner.owner_kind
        ));
    }
    let source = "import { Cart } from '../src/lib';\ntest('p', () => {\n  const helper = new Cart();\n  const r = helper.price(150);\n  expect(r).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "instance method alias initializer must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// Static method owners keep the same member-segment credit (`Cart.price`).
#[test]
fn issue7813_static_method_alias_initializer_stays_exposed() -> Result<(), String> {
    let lib = class_static_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    if !matches!(owner.owner_kind, OwnerKind::ClassMethod) {
        return Err(format!(
            "static method owner must be ClassMethod, got {:?}",
            owner.owner_kind
        ));
    }
    let source = "import { Cart } from '../src/lib';\ntest('p', () => {\n  const r = Cart.price(150);\n  expect(r).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "static method alias initializer must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

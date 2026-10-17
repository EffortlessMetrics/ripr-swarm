//! Issue #7815: mixed typeof-plus-value assertions must not lose all credit.
//!
//! `ts_observed_is_typeof_name` discarded the whole assertion when any
//! typeof owner reference was present, so
//! `expect(typeof price === 'function' ? price(150) : 0).toBe(130)` lost
//! its genuine value check. The repair excludes only typeof-consumed
//! occurrences; a later bare owner call in the same expression still
//! observes the changed sink. Pure typeof observations stay non-value.
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

/// The issue repro: a typeof guard in the same assertion as a genuine
/// `price(150)` value check must not discard the value witness.
#[test]
fn issue7815_mixed_typeof_plus_value_stays_exposed() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    if !matches!(owner.owner_kind, OwnerKind::Function) {
        return Err(format!(
            "bare-call owner must be a function, got {:?}",
            owner.owner_kind
        ));
    }
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  expect(typeof price === 'function' ? price(150) : 0).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof price === 'function' ? price(150) : 0")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "mixed typeof-plus-value assertion must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// Parenthesized `typeof(price)` is the same type observation; the later
/// bare call still witnesses the changed sink.
#[test]
fn issue7815_parenthesized_typeof_plus_value_stays_exposed() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  expect(typeof(price) === 'function' ? price(150) : 0).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof(price) === 'function' ? price(150) : 0")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "parenthesized typeof-plus-value must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// One-hop alias of a mixed typeof-plus-value initializer still observes
/// the genuine owner call on the RHS.
#[test]
fn issue7815_mixed_typeof_alias_initializer_stays_exposed() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  const r = typeof price === 'function' ? price(150) : 0;\n  expect(r).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "mixed typeof-plus-value alias must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// A namespace import still observes through `ns.price(` after a typeof
/// guard on the same namespaced member.
#[test]
fn issue7815_namespaced_typeof_plus_value_stays_exposed() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import * as ns from '../src/lib';\ntest('p', () => {\n  expect(typeof ns.price === 'function' ? ns.price(150) : 0).toBe(130);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof ns.price === 'function' ? ns.price(150) : 0")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if !matches!(finding.class, ExposureClass::Exposed) {
        return Err(format!(
            "namespaced typeof-plus-value must stay exposed, got {:?}",
            finding.class
        ));
    }
    Ok(())
}

/// Preservation: a typeof observation with no value check still withholds.
#[test]
fn issue7815_typeof_only_stays_non_value() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect(typeof price).toBe(\"function\");\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof price")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("typeof-only observation must not read exposed".to_string());
    }
    Ok(())
}

/// A typeof comparison without a later owner value check is still a type
/// string, not a ReturnValue witness.
#[test]
fn issue7815_typeof_comparison_only_stays_non_value() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect(typeof price === 'function').toBe(true);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof price === 'function'")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("typeof comparison must not read exposed".to_string());
    }
    Ok(())
}

/// `typeof price(150)` observes the type of the return, not the returned
/// number, so it must not confirm the discount sink.
#[test]
fn issue7815_typeof_of_call_stays_non_value() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  expect(typeof price(150)).toBe('number');\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof price(150)")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("typeof of the owner call must not read exposed".to_string());
    }
    Ok(())
}

/// The value branch must still be the owner. `helper.price(150)` is another
/// receiver's member, so mixing it with `typeof price` must not invent credit.
#[test]
fn issue7815_mixed_typeof_plus_member_stays_non_value() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150); expect(typeof price === 'function' ? helper.price(150) : 0).toBe(140);\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(
        &tests,
        "typeof price === 'function' ? helper.price(150) : 0",
    )?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("typeof plus another receiver's member must not read exposed".to_string());
    }
    Ok(())
}

/// `typeof ns.price(150)` observes the type of the return, not the returned
/// number. The namespaced call matcher must not treat the typeof operand as
/// a value witness (the dual of `issue7815_typeof_of_call_stays_non_value`).
#[test]
fn issue7815_namespaced_typeof_of_call_stays_non_value() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import * as ns from '../src/lib';\ntest('p', () => {\n  expect(typeof ns.price(150)).toBe('number');\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "typeof ns.price(150)")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("namespaced typeof of the owner call must not read exposed".to_string());
    }
    Ok(())
}

/// Aliasing a typeof-only initializer must not invent a value witness.
#[test]
fn issue7815_typeof_only_alias_stays_non_value() -> Result<(), String> {
    let lib = named_lib();
    let owners = owners_of_lib(&lib);
    let owner = price_owner_named(&owners)?;
    let source = "import { price } from '../src/lib';\ntest('p', () => {\n  price(150);\n  const r = typeof price;\n  expect(r).toBe('function');\n});\n";
    let tests = tests_of(source);
    assert_single_strong_exact(&tests, "r")?;
    assert_trusted_relation(owner, &tests)?;
    let finding = classify_at_changed(&lib, &owners, &tests)?;
    if matches!(finding.class, ExposureClass::Exposed) {
        return Err("typeof-only alias initializer must not read exposed".to_string());
    }
    Ok(())
}

//! RIPR-SPEC-0243 rules 5, 6 and 7 over parsed owners and tests.
//!
//! - Rule 5: a default import anchors only a default-exported owner.
//! - Rule 6: value-family owner and changed-token references match whole
//!   identifiers.
//! - Rule 7: an alias-rename local counts as the owner in the value guard;
//!   RIPR-SPEC-0251 extends it to a renamed default import of the owner.
//!
//! Each case asserts the parsed subject (owner facts, import record,
//! relation, assertion) before the class, so a fixture that stops parsing
//! the intended shape fails here rather than passing for the wrong reason.

use super::*;

const LIB_FILE: &str = "src/lib.ts";
const TEST_FILE: &str = "tests/lib.test.ts";
const CHANGED_LINE: usize = 3;
const CHANGED_TEXT: &str = "    return amount - 20;";

const PRICE_BODY: &str = "(amount: number): number {\n  if (amount > 100) {\n    return amount - 20;\n  }\n  return amount;\n}\n";

fn price_lib(prefix: &str, suffix: &str) -> String {
    format!("{prefix}function price{PRICE_BODY}{suffix}")
}

fn owners_of(source: &str) -> Vec<TypeScriptOwner> {
    extract_owners(Path::new(LIB_FILE), source)
}

fn price_owner(owners: &[TypeScriptOwner]) -> Result<&TypeScriptOwner, String> {
    owners
        .iter()
        .find(|owner| owner.name == "price")
        .ok_or_else(|| format!("no `price` owner in {owners:?}"))
}

fn tests_of(source: &str) -> Vec<TypeScriptTest> {
    extract_tests(Path::new(TEST_FILE), source)
}

fn relations(owner: &TypeScriptOwner, tests: &[TypeScriptTest]) -> Vec<TypeScriptRelationKind> {
    related_test_candidates(
        owner,
        tests,
        None,
        &ReExportIndex::empty(),
        None,
        &RelationIndex::empty(),
    )
    .into_iter()
    .map(|candidate| candidate.relation)
    .collect()
}

fn classify(owners: &[TypeScriptOwner], tests: &[TypeScriptTest]) -> Result<Finding, String> {
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

/// The single strong `toBe` assertion of the single test, with its observed
/// expression.
fn only_observed(tests: &[TypeScriptTest]) -> Result<String, String> {
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
    assertion
        .observed_expression
        .clone()
        .ok_or_else(|| "assertion has no observed expression".to_string())
}

fn has_propagation_unknown(finding: &Finding) -> bool {
    finding
        .missing
        .iter()
        .any(|line| line.contains("propagation_unknown"))
}

// ── Rule 5 ───────────────────────────────────────────────────────────────

/// Example 16: `import price from "../src/lib"` binds the module's default
/// export `round`, not the named owner `price`, so the bare `price(` call is
/// not an owner call and no test reaches the owner.
#[test]
fn rule5_ex16_default_import_of_other_default_export_is_not_an_owner_call() -> Result<(), String> {
    let lib = price_lib(
        "export ",
        "\nexport default function round(n: number): number {\n  return Math.round(n);\n}\n",
    );
    let owners = owners_of(&lib);
    let owner = price_owner(&owners)?;
    assert!(!owner.exported_as_default, "price is a named export");
    let tests = tests_of(
        "import price from \"../src/lib\";\n\ntest(\"x\", () => {\n  expect(price(150)).toBe(130);\n});\n",
    );
    let [test] = tests.as_slice() else {
        return Err(format!("expected one test, got {tests:?}"));
    };
    assert!(
        test.imports_in_file
            .iter()
            .any(|import| import.local == "price"
                && import.imported.as_deref() == Some("default")
                && !import.namespace),
        "default import record: {:?}",
        test.imports_in_file
    );
    assert_eq!(only_observed(&tests)?, "price(150)");
    assert_eq!(relations(owner, &tests), Vec::new());

    let finding = classify(&owners, &tests)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    Ok(())
}

/// Example 17 positive control: the owner IS the default export, so the
/// default import under its name anchors the bare call.
#[test]
fn rule5_ex17_default_import_of_default_owner_stays_direct_owner_call() -> Result<(), String> {
    let owners = owners_of(&price_lib("export default ", ""));
    let owner = price_owner(&owners)?;
    assert!(owner.exported_as_default);
    let tests = tests_of(
        "import price from \"../src/lib\";\n\ntest(\"price\", () => {\n  expect(price(150)).toBe(130);\n});\n",
    );
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::DirectOwnerCall]
    );
    assert_eq!(classify(&owners, &tests)?.class, ExposureClass::Exposed);
    Ok(())
}

/// A separate `export default price;` or `export { price as default }`
/// statement makes the owner the default export too; rule 5 must not drop
/// that anchor.
#[test]
fn rule5_default_export_by_binding_marks_owner_and_anchors() -> Result<(), String> {
    for suffix in [
        "\nexport default price;\n",
        "\nexport { price as default };\n",
    ] {
        let owners = owners_of(&price_lib("", suffix));
        let owner = price_owner(&owners)?;
        assert!(owner.exported_as_default, "suffix {suffix:?}");
        let tests = tests_of(
            "import price from \"../src/lib\";\n\ntest(\"price\", () => {\n  expect(price(150)).toBe(130);\n});\n",
        );
        assert_eq!(
            relations(owner, &tests),
            vec![TypeScriptRelationKind::DirectOwnerCall],
            "suffix {suffix:?}"
        );
        assert_eq!(classify(&owners, &tests)?.class, ExposureClass::Exposed);
    }
    Ok(())
}

/// A different binding exported as default does not make the owner the
/// default export.
#[test]
fn rule5_default_export_of_other_binding_does_not_mark_owner() -> Result<(), String> {
    let owners = owners_of(&price_lib(
        "export ",
        "\nconst other = 1;\nexport { other as default };\n",
    ));
    assert!(!price_owner(&owners)?.exported_as_default);
    Ok(())
}

/// A namespace binding is never an anchor for a bare call.
#[test]
fn rule5_namespace_binding_does_not_anchor_bare_call() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import * as price from \"../src/lib\";\n\ntest(\"x\", () => {\n  expect(price(150)).toBe(130);\n});\n",
    );
    assert!(
        !relations(owner, &tests).contains(&TypeScriptRelationKind::DirectOwnerCall),
        "namespace binding must not yield DirectOwnerCall"
    );
    Ok(())
}

// ── Rule 6 ───────────────────────────────────────────────────────────────

#[test]
fn rule6_whole_identifier_reference_boundaries() {
    assert!(!ts_references_identifier("address", "add"));
    assert!(!ts_references_identifier("totalAmount", "amount"));
    assert!(!ts_references_identifier("$price", "price"));
    // Non-ASCII identifier characters are part of the identifier too.
    assert!(!ts_references_identifier("éprice", "price"));
    assert!(!ts_references_identifier("priceé", "price"));
    assert!(!ts_references_identifier("price_", "price"));
    assert!(!ts_references_identifier("price", ""));
    assert!(ts_references_identifier("ns.price(150)", "price"));
    assert!(ts_references_identifier("c.total()", "total"));
    assert!(ts_references_identifier("add(1, 2)", "add"));
    assert!(ts_references_identifier("addr + add", "add"));
    assert!(ts_references_identifier("amount", "amount"));
}

/// Example 22: the asserted local `address` only shares the owner name `add`
/// as a prefix; the observation guard must not credit it.
#[test]
fn rule6_ex22_prefix_named_local_is_not_an_owner_observation() -> Result<(), String> {
    let lib = "export function add(a: number, b: number): number {\n  return a - b;\n}\n";
    let owners = extract_owners(Path::new("src/add.ts"), lib);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "add")
        .ok_or_else(|| format!("no add owner: {owners:?}"))?;
    let tests = extract_tests(
        Path::new("tests/add.test.ts"),
        "import { add } from \"../src/add\";\n\ntest(\"add\", () => {\n  const address = \"x\";\n  add(1, 2);\n  expect(address).toBe(\"x\");\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "address");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::DirectOwnerCall]
    );
    let finding = classify_change(
        Path::new("src/add.ts"),
        2,
        "  return a - b;",
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "expected a finding".to_string())?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(has_propagation_unknown(&finding), "{:?}", finding.missing);
    Ok(())
}

/// `totalAmount` does not contain the changed token `amount`.
#[test]
fn rule6_changed_token_inside_larger_identifier_is_not_observed() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import { price } from \"../src/lib\";\n\ntest(\"price\", () => {\n  const totalAmount = 5;\n  price(150);\n  expect(totalAmount).toBe(5);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "totalAmount");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::DirectOwnerCall]
    );
    let finding = classify(&owners, &tests)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(has_propagation_unknown(&finding), "{:?}", finding.missing);
    Ok(())
}

/// A member segment counts: `ns.price(150)` references `price`.
#[test]
fn rule6_namespace_member_call_still_references_owner() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import * as ns from \"../src/lib\";\n\ntest(\"price\", () => {\n  expect(ns.price(150)).toBe(130);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "ns.price(150)");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::ImportedOwnerCall]
    );
    assert_eq!(classify(&owners, &tests)?.class, ExposureClass::Exposed);
    Ok(())
}

/// Example 20: the one-hop local credit survives the whole-identifier test.
#[test]
fn rule6_ex20_owner_initialized_local_stays_exposed() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let tests = tests_of(
        "import { price } from \"../src/lib\";\n\ntest(\"price\", () => {\n  const r = price(150);\n  expect(r).toBe(130);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "r");
    assert_eq!(classify(&owners, &tests)?.class, ExposureClass::Exposed);
    Ok(())
}

// ── Rule 7 ───────────────────────────────────────────────────────────────

/// Example 18: `import { price as p }` and `expect(p(150)).toBe(130)`.
#[test]
fn rule7_ex18_alias_local_call_is_an_owner_observation() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import { price as p } from \"../src/lib\";\n\ntest(\"price\", () => {\n  expect(p(150)).toBe(130);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "p(150)");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::ImportAliasOwnerCall]
    );
    let finding = classify(&owners, &tests)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(!has_propagation_unknown(&finding), "{:?}", finding.missing);
    Ok(())
}

/// Example 19: the one-hop initializer accepts the alias local.
#[test]
fn rule7_ex19_alias_local_initializer_is_an_owner_observation() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import { price as p } from \"../src/lib\";\n\ntest(\"price\", () => {\n  const r = p(150);\n  expect(r).toBe(130);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "r");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::ImportAliasOwnerCall]
    );
    assert_eq!(classify(&owners, &tests)?.class, ExposureClass::Exposed);
    Ok(())
}

/// The alias local matches as a whole identifier only: asserting `pp` after
/// calling `p(150)` observes nothing of the owner.
#[test]
fn rule7_alias_local_prefix_is_not_an_owner_observation() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import { price as p } from \"../src/lib\";\n\ntest(\"price\", () => {\n  const pp = 1;\n  p(150);\n  expect(pp).toBe(1);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "pp");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::ImportAliasOwnerCall]
    );
    let finding = classify(&owners, &tests)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(has_propagation_unknown(&finding), "{:?}", finding.missing);
    Ok(())
}

/// An alias name spelled inside a string is not a reference: calling
/// `p(150)` and asserting `expect('p').toBe('p')` observes nothing of the
/// owner, directly or through a string-initialized local.
#[test]
fn rule7_alias_name_inside_a_string_is_not_an_owner_observation() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    for (body, observed) in [
        ("  p(150);\n  expect('p').toBe('p');\n", "'p'"),
        ("  p(150);\n  expect(`p ${1}`).toBe('p 1');\n", "`p ${1}`"),
        (
            "  const r = 'p'; // p(1)\n  p(150);\n  expect(r).toBe('p');\n",
            "r",
        ),
    ] {
        let tests = tests_of(&format!(
            "import {{ price as p }} from \"../src/lib\";\n\ntest(\"price\", () => {{\n{body}}});\n"
        ));
        assert_eq!(only_observed(&tests)?, observed);
        assert_eq!(
            relations(owner, &tests),
            vec![TypeScriptRelationKind::ImportAliasOwnerCall]
        );
        let finding = classify(&owners, &tests)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{body}");
        assert!(has_propagation_unknown(&finding), "{:?}", finding.missing);
    }
    Ok(())
}

#[test]
fn literal_and_comment_masking_keeps_code_identifiers() {
    let masked = ts_mask_literals_and_comments("f('p') + g(\"p\\\"p\") /* p */ + `p${p(1)}p` // p");
    assert!(!masked.contains("'p'"));
    assert_eq!(masked.matches('p').count(), 1, "{masked}");
    assert!(ts_references_identifier(&masked, "p"));
    assert!(ts_references_identifier(&masked, "f"));
    assert!(ts_references_identifier(&masked, "g"));
    assert_eq!(
        masked.chars().count(),
        "f('p') + g(\"p\\\"p\") /* p */ + `p${p(1)}p` // p"
            .chars()
            .count()
    );
}

/// A regex literal is masked as a whole, so a quote or `//` inside it does
/// not open a string or comment that would hide a later owner reference;
/// division stays code.
#[test]
fn literal_masking_reads_regex_literals_and_division() {
    for (text, ident) in [
        ("/^https:\\/\\//.test(buildUrl(x))", "buildUrl"),
        ("/\"/.test(quote(s))", "quote"),
        ("s.split(/'/).map(p)", "p"),
        ("expect(/[/'\"]/g.test(p(1)))", "p"),
        ("a / b / p(1)", "p"),
        ("f(x) / 2 + p(1)", "p"),
        ("typeof /\"/.test(s) && quote(s)", "quote"),
        ("await /'/.exec(p(1))", "p"),
        ("x.return / 2 + p(1)", "p"),
        ("total / 2 + p(1)", "p"),
        // #7362: an apostrophe in JSX text is not a string.
        ("render(<b>Don't {p(1)}</b>)", "p"),
        ("render(<b>it's \"x\" {p(1)}</b>)", "p"),
        ("render(<b>Hi. \"Bye {p(1)}</b>)", "p"),
    ] {
        let masked = ts_mask_literals_and_comments(text);
        assert!(
            ts_references_identifier(&masked, ident),
            "{text} -> {masked}"
        );
        assert_eq!(masked.chars().count(), text.chars().count(), "{text}");
    }
    // Strings still open after an operator, `(` or a keyword.
    for text in [
        "f('p')",
        "x = \"p\"",
        "return 'p'",
        "<a title='p'>",
        "x as 'p'",
        "x satisfies 'p'",
        "import q from 'p'",
        "f(x) as 'p'",
        "[...'p']",
        "[... 'p']",
        "(x): x is 'p'",
        "declare module 'p'",
        "class C { static 'p'() {} }",
        "class C { get 'p'() {} }",
        "f(/* c */ 'p')",
        "x = // c\n'p'",
        "return /* c */ 'p'",
    ] {
        let masked = ts_mask_literals_and_comments(text);
        assert!(
            !ts_references_identifier(&masked, "p"),
            "{text} -> {masked}"
        );
    }
    let masked = ts_mask_literals_and_comments("expect(/price/i.test(s))");
    assert!(!ts_references_identifier(&masked, "price"), "{masked}");
    assert!(!ts_references_identifier(&masked, "i"), "{masked}");
    assert!(ts_references_identifier(&masked, "s"), "{masked}");
}

/// RIPR-SPEC-0251 rule 1 (amends the RIPR-SPEC-0243 Example 17 renamed
/// non-goal): a renamed default import of the module whose default export
/// is the owner counts as the owner in the value guard.
#[test]
fn rule7_default_import_rename_counts_as_the_owner() -> Result<(), String> {
    let owners = owners_of(&price_lib("export default ", ""));
    let owner = price_owner(&owners)?;
    assert!(owner.exported_as_default, "price is the default export");
    let tests = tests_of(
        "import cost from \"../src/lib\";\n\ntest(\"price\", () => {\n  expect(cost(150)).toBe(130);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "cost(150)");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::ImportedOwnerCall]
    );
    let finding = classify(&owners, &tests)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(!has_propagation_unknown(&finding), "{:?}", finding.missing);
    Ok(())
}

/// RIPR-SPEC-0251 example 6: a local bound once to the renamed default
/// import's call observes the owner, as RIPR-SPEC-0243 example 19 reads an
/// alias rename.
#[test]
fn rule7_default_import_local_initializer_is_an_owner_observation() -> Result<(), String> {
    let owners = owners_of(&price_lib("export default ", ""));
    let owner = price_owner(&owners)?;
    let tests = tests_of(
        "import cost from \"../src/lib\";\n\ntest(\"price\", () => {\n  const r = cost(150);\n  expect(r).toBe(130);\n});\n",
    );
    assert_eq!(only_observed(&tests)?, "r");
    assert_eq!(
        relations(owner, &tests),
        vec![TypeScriptRelationKind::ImportedOwnerCall]
    );
    assert_eq!(classify(&owners, &tests)?.class, ExposureClass::Exposed);
    Ok(())
}

/// RIPR-SPEC-0251 negatives: the renamed default import earns nothing when
/// the owner is not the module's default export, when the test body
/// redeclares the local, or when the strong assertion observes another
/// callee. The first two are refused at the relation layer (no relation);
/// the third keeps `ImportedOwnerCall` and is refused by the value guard.
#[test]
fn rule7_default_import_rename_negatives_keep_no_credit() -> Result<(), String> {
    let named_lib = price_lib(
        "export ",
        "\nexport default function round(n: number): number {\n  return Math.round(n);\n}\n",
    );
    let default_lib = price_lib("export default ", "");
    let cases = [
        (
            "owner is a named export",
            named_lib.as_str(),
            "import cost from \"../src/lib\";\n\ntest(\"price\", () => {\n  expect(cost(150)).toBe(130);\n});\n",
            "cost(150)",
            Vec::new(),
        ),
        (
            "test body redeclares the local",
            default_lib.as_str(),
            "import cost from \"../src/lib\";\n\ntest(\"price\", () => {\n  cost(1);\n  const cost = (n: number) => n - 20;\n  expect(cost(150)).toBe(130);\n});\n",
            "cost(150)",
            Vec::new(),
        ),
        (
            "test body destructures the local from a helper",
            default_lib.as_str(),
            "import cost from \"../src/lib\";\nconst helpers = { cost: (n: number) => 140 };\n\ntest(\"price\", () => {\n  const { cost } = helpers;\n  expect(cost(150)).toBe(140);\n});\n",
            "cost(150)",
            vec![TypeScriptRelationKind::ImportedOwnerCall],
        ),
        (
            "assertion observes another callee",
            default_lib.as_str(),
            "import cost from \"../src/lib\";\nimport other from \"../src/other\";\n\ntest(\"price\", () => {\n  cost(1);\n  expect(other(150)).toBe(130);\n});\n",
            "other(150)",
            vec![TypeScriptRelationKind::ImportedOwnerCall],
        ),
    ];
    for (label, lib, test_source, observed, expected_relations) in cases {
        let owners = owners_of(lib);
        let tests = tests_of(test_source);
        assert_eq!(only_observed(&tests)?, observed, "{label}");
        assert_eq!(
            relations(price_owner(&owners)?, &tests),
            expected_relations,
            "{label}"
        );
        let finding = classify(&owners, &tests)?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {finding:?}"
        );
    }
    Ok(())
}

/// RIPR-SPEC-0251 rule 1 conditions on the helper itself: each case breaks
/// one condition the relation layer may also refuse, so the helper stays
/// fail-closed when it is reached by another relation path.
#[test]
fn default_import_owner_locals_keeps_only_the_owner_binding() -> Result<(), String> {
    let default_lib = price_lib("export default ", "");
    let named_lib = price_lib("export ", "");
    let default_owners = owners_of(&default_lib);
    let named_owners = owners_of(&named_lib);
    let default_owner = price_owner(&default_owners)?;
    let named_owner = price_owner(&named_owners)?;
    let body = |imports: &str, lines: &str| {
        format!("{imports}\n\ntest(\"price\", () => {{\n{lines}\n}});\n")
    };
    let cases: [(&str, &TypeScriptOwner, String, Vec<&str>); 15] = [
        (
            "renamed default import of the owner",
            default_owner,
            body("import cost from \"../src/lib\";", "  expect(cost(150)).toBe(130);"),
            vec!["cost"],
        ),
        (
            "owner is not the default export",
            named_owner,
            body("import cost from \"../src/lib\";", "  expect(cost(150)).toBe(130);"),
            Vec::new(),
        ),
        (
            "default import of another module",
            default_owner,
            body("import cost from \"../src/other\";", "  expect(cost(150)).toBe(130);"),
            Vec::new(),
        ),
        (
            "local is the owner's own name",
            default_owner,
            body("import price from \"../src/lib\";", "  expect(price(150)).toBe(130);"),
            Vec::new(),
        ),
        (
            "local is never called",
            default_owner,
            body("import cost from \"../src/lib\";", "  expect(cost).toBeDefined();"),
            Vec::new(),
        ),
        (
            "test body redeclares the local",
            default_owner,
            body(
                "import cost from \"../src/lib\";",
                "  const cost = (n: number) => n;\n  expect(cost(150)).toBe(130);",
            ),
            Vec::new(),
        ),
        (
            "test body destructures the local",
            default_owner,
            body(
                "import cost from \"../src/lib\";",
                "  const { cost } = helpers;\n  expect(cost(150)).toBe(130);",
            ),
            Vec::new(),
        ),
        (
            "test body destructures the local under a renamed key",
            default_owner,
            body(
                "import cost from \"../src/lib\";",
                "  const { price: cost } = helpers;\n  expect(cost(150)).toBe(130);",
            ),
            Vec::new(),
        ),
        (
            "test body array-destructures the local",
            default_owner,
            body(
                "import cost from \"../src/lib\";",
                "  const [cost] = fns;\n  expect(cost(150)).toBe(130);",
            ),
            Vec::new(),
        ),
        (
            "test body destructures the local over several lines",
            default_owner,
            body(
                "import cost from \"../src/lib\";",
                "  const {\n    cost\n  } = helpers;\n  expect(cost(150)).toBe(130);",
            ),
            Vec::new(),
        ),
        (
            "test body destructures the local with a default initializer",
            default_owner,
            body(
                "import cost from \"../src/lib\";",
                "  const { cost = fallback } = helpers;\n  expect(cost(150)).toBe(130);",
            ),
            Vec::new(),
        ),
        (
            "top-level require binds the module, not the default",
            default_owner,
            body("const cost = require(\"../src/lib\");", "  expect(cost(150)).toBe(130);"),
            Vec::new(),
        ),
        (
            "named import, not the default",
            default_owner,
            body("import { cost } from \"../src/lib\";", "  expect(cost(150)).toBe(130);"),
            Vec::new(),
        ),
        (
            "enclosing describe redeclares the local",
            default_owner,
            "import cost from \"../src/lib\";\n\ndescribe(\"d\", () => {\n  const cost = (n: number) => n;\n  test(\"price\", () => {\n    expect(cost(150)).toBe(130);\n  });\n});\n".to_string(),
            Vec::new(),
        ),
        (
            "namespace import",
            default_owner,
            body("import * as cost from \"../src/lib\";", "  expect(cost.default(150)).toBe(130);"),
            Vec::new(),
        ),
    ];
    for (label, owner, source, expected) in cases {
        let tests = tests_of(&source);
        let [test] = tests.as_slice() else {
            return Err(format!("{label}: expected one test, got {}", tests.len()));
        };
        assert_eq!(
            default_import_owner_locals(test, owner, None, None),
            expected,
            "{label}: {:?}",
            test.imports_in_file
        );
    }
    Ok(())
}

/// The rule 7 alias-rename local is shadowed by a body-local destructuring
/// too: `const { p } = helpers` rebinds `p`, so `expect(p(150))` observes
/// the helper, not the owner.
#[test]
fn rule7_alias_local_destructured_in_the_body_is_not_the_owner() -> Result<(), String> {
    let owners = owners_of(&price_lib("export ", ""));
    let owner = price_owner(&owners)?;
    for (label, pattern) in [
        ("shorthand", "const { p } = helpers;"),
        ("renamed key", "const { price: p } = helpers;"),
        ("array", "const [p] = fns;"),
        ("multi-line", "const {\n    p\n  } = helpers;"),
        ("default initializer", "const { p = fallback } = helpers;"),
        (
            "earlier default initializer",
            "const { a = 1, p } = helpers;",
        ),
        (
            "multi-line renamed key",
            "let {\n    price: p,\n  } = helpers;",
        ),
    ] {
        let tests = tests_of(&format!(
            "import {{ price as p }} from \"../src/lib\";\n\ntest(\"price\", () => {{\n  {pattern}\n  expect(p(150)).toBe(140);\n}});\n"
        ));
        assert_eq!(only_observed(&tests)?, "p(150)", "{label}");
        let [test] = tests.as_slice() else {
            return Err(format!("{label}: expected one test"));
        };
        assert!(
            import_alias_owner_locals(test, owner, None, None).is_empty(),
            "{label}"
        );
        assert_ne!(
            classify(&owners, &tests)?.class,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    Ok(())
}

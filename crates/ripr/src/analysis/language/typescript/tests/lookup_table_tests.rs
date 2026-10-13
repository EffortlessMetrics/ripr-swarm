//! RIPR-SPEC-0254: a changed entry of a lookup table the owner returns from
//! is credited only when a strong assertion looks up a changed key.

use super::*;

const OLD: &str = "  const table: Record<string, number> = { apple: 120, pear: 95 };";
const NEW: &str = "  const table: Record<string, number> = { apple: 120, pear: 99 };";

fn cart_source(table_line: &str, ret: &str) -> String {
    format!(
        "export async function fetchUnitPrice(sku: string): Promise<number> {{\n{table_line}\n{ret}\n}}\n"
    )
}

fn unit_price_owner(source: &str) -> Result<TypeScriptOwner, String> {
    extract_owners(Path::new("src/cart.ts"), source)
        .into_iter()
        .find(|owner| owner.name == "fetchUnitPrice")
        .ok_or_else(|| format!("no `fetchUnitPrice` owner in {source}"))
}

#[test]
fn lookup_table_change_reads_the_moved_keys_and_the_key_parameter() -> Result<(), String> {
    let owner = unit_price_owner(&cart_source(NEW, "  return table[sku] ?? 0;"))?;
    assert_eq!(owner.params, vec!["sku".to_string()]);
    assert_eq!(
        ts_lookup_table_change(&owner, NEW, Some(OLD)),
        Some(LookupTableChange {
            param_index: 0,
            changed_keys: vec!["pear".to_string()],
        })
    );
    // Quoted keys, `let`, no annotation, a plain return and string values.
    let new = "  let table = { \"a-1\": 'x', b: 'y' };";
    let old = "  let table = { \"a-1\": 'x', b: 'z' };";
    let owner = unit_price_owner(&cart_source(new, "  return table[sku];"))?;
    assert_eq!(
        ts_lookup_table_change(&owner, new, Some(old)),
        Some(LookupTableChange {
            param_index: 0,
            changed_keys: vec!["b".to_string()],
        })
    );
    // A one-letter table name, a decimal-integer key and a move to `null`
    // with no fallback.
    let new = "  const t = { 10: null, b: 2 };";
    let old = "  const t = { 10: 1, b: 2 };";
    let owner = unit_price_owner(&cart_source(new, "  return t[sku];"))?;
    assert_eq!(
        ts_lookup_table_change(&owner, new, Some(old)),
        Some(LookupTableChange {
            param_index: 0,
            changed_keys: vec!["10".to_string()],
        })
    );
    // Comments and strings are masked before the guard counts `return`,
    // `function` and the table name.
    let owner = unit_price_owner(&cart_source(
        NEW,
        "  // return table, a function of sku\n  /* return */\n  log(\"table function return\");\n  return table[sku] ?? 0;",
    ))?;
    assert_eq!(
        ts_lookup_table_change(&owner, NEW, Some(OLD)),
        Some(LookupTableChange {
            param_index: 0,
            changed_keys: vec!["pear".to_string()],
        })
    );
    // A move to or from `null` under a literal fallback counts when the
    // returned values still differ: `95` then the fallback `0` or `null`.
    for (new, old, ret) in [
        (
            "  const table = { apple: 120, pear: null };",
            OLD,
            "  return table[sku] ?? 0;",
        ),
        (
            "  const table = { apple: 120, pear: null };",
            OLD,
            "  return table[sku] ?? null;",
        ),
        (
            OLD,
            "  const table = { apple: 120, pear: null };",
            "  return table[sku] ?? 0;",
        ),
    ] {
        let owner = unit_price_owner(&cart_source(new, ret))?;
        assert_eq!(
            ts_lookup_table_change(&owner, new, Some(old)),
            Some(LookupTableChange {
                param_index: 0,
                changed_keys: vec!["pear".to_string()],
            }),
            "{new} / {old} / {ret}"
        );
    }
    // The key parameter need not be the first.
    let source = format!(
        "export function fetchUnitPrice(region: string, sku: string): number {{\n{NEW}\n  return table[sku] ?? 0;\n}}\n"
    );
    let owner = unit_price_owner(&source)?;
    assert_eq!(
        ts_lookup_table_change(&owner, NEW, Some(OLD)),
        Some(LookupTableChange {
            param_index: 1,
            changed_keys: vec!["pear".to_string()],
        })
    );
    Ok(())
}

#[test]
fn lookup_table_change_refuses_other_shapes() -> Result<(), String> {
    let cases: [(&str, &str, &str, &str); 35] = [
        ("no old line", NEW, "", "  return table[sku] ?? 0;"),
        (
            "a key added",
            "  const table = { apple: 120, pear: 99, fig: 1 };",
            "  const table = { apple: 120, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "keys reordered",
            "  const table = { pear: 95, apple: 121 };",
            "  const table = { apple: 120, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "no value moved",
            "  const table = { apple: 120, pear: 95.0 };",
            "  const table = { apple: 120, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a nested value",
            "  const table = { apple: 120, pear: [95] };",
            "  const table = { apple: 120, pear: [96] };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a computed value",
            "  const table = { apple: 120, pear: base + 1 };",
            "  const table = { apple: 120, pear: base + 2 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a duplicate key",
            "  const table = { pear: 120, pear: 99 };",
            "  const table = { pear: 120, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a fallback reading the table",
            NEW,
            OLD,
            "  return table[sku] ?? table.apple;",
        ),
        (
            "an identifier value that looks like a number",
            "  const table = { apple: 120, pear: 2 };",
            "  const table = { apple: 120, pear: _1 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a number moved to its own numeric string",
            "  const table = { apple: 120, pear: \"95\" };",
            "  const table = { apple: 120, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a boolean moved to a number",
            "  const table = { apple: 120, pear: 1 };",
            "  const table = { apple: 120, pear: true };",
            "  return table[sku] ?? 0;",
        ),
        (
            "an integer key that may round",
            "  const table = { 12345678901234567890: 5, b: 2 };",
            "  const table = { 12345678901234567890: 6, b: 2 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a fallback that starts with a string",
            NEW,
            OLD,
            "  return table[sku] ?? \"x\" ? 1 : 2;",
        ),
        (
            "a fallback followed by a comma",
            NEW,
            OLD,
            "  return table[sku] ?? \"x\", 0;",
        ),
        (
            "a nested function",
            NEW,
            OLD,
            "  function helper() {}\n  return table[sku] ?? 0;",
        ),
        (
            "the table is read by another statement",
            NEW,
            OLD,
            "  if (table.apple > 100) log(\"big\");\n  return table[sku] ?? 0;",
        ),
        (
            "only the quote style moved",
            "  const table = { apple: 'x', pear: \"y\" };",
            "  const table = { apple: 'x', pear: 'y' };",
            "  return table[sku];",
        ),
        (
            "an exponent key",
            "  const table = { 1e3: 6, pear: 95 };",
            "  const table = { 1e3: 5, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a hexadecimal key",
            "  const table = { 0x10: 6, pear: 95 };",
            "  const table = { 0x10: 5, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a `__proto__` key",
            "  const table = { __proto__: 6, pear: 95 };",
            "  const table = { __proto__: 5, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a quoted `__proto__` key",
            "  const table = { \"__proto__\": 6, pear: 95 };",
            "  const table = { \"__proto__\": 5, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a key with a space",
            "  const table = { \"a b\": 6, pear: 95 };",
            "  const table = { \"a b\": 5, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a concatenated string value",
            "  const table = { apple: \"a\" + x + \"c\", pear: \"p\" };",
            "  const table = { apple: \"a\" + x + \"b\", pear: \"p\" };",
            "  return table[sku];",
        ),
        (
            "a move to `null` under a fallback",
            "  const table = { apple: 120, pear: null };",
            "  const table = { apple: 120, pear: 0 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a move to `null` under a string fallback that may equal it",
            "  const table = { apple: \"a\", pear: null };",
            "  const table = { apple: \"a\", pear: \"b\" };",
            "  return table[sku] ?? \"b\";",
        ),
        (
            "a move to `null` under an `undefined` fallback",
            "  const table = { apple: 120, pear: null };",
            "  const table = { apple: 120, pear: 95 };",
            "  return table[sku] ?? undefined;",
        ),
        (
            "a number moved to a BigInt",
            "  const table = { apple: 120, pear: 95n };",
            "  const table = { apple: 120, pear: 95 };",
            "  return table[sku] ?? 0;",
        ),
        (
            "a BigInt value moved",
            "  const table = { apple: 120, pear: 96n };",
            "  const table = { apple: 120, pear: 95n };",
            "  return table[sku] ?? 0;",
        ),
        (
            "another table name",
            "  const prices = { apple: 120, pear: 99 };",
            "  const table = { apple: 120, pear: 95 };",
            "  return prices[sku] ?? 0;",
        ),
        (
            "a nested arrow",
            NEW,
            OLD,
            "  const pick = (k: string) => k;\n  return table[sku] ?? 0;",
        ),
        (
            "a second return",
            NEW,
            OLD,
            "  if (!sku) return 0;\n  return table[sku] ?? 0;",
        ),
        ("a member read", NEW, OLD, "  return table.pear;"),
        (
            "a key that is not a parameter",
            NEW,
            OLD,
            "  return table[sku.trim()] ?? 0;",
        ),
        (
            "a rebound key parameter",
            NEW,
            OLD,
            "  sku = sku.toLowerCase();\n  return table[sku] ?? 0;",
        ),
        (
            "a computed fallback",
            NEW,
            OLD,
            "  return table[sku] ?? lookupRemote(sku);",
        ),
    ];
    for (label, new, old, ret) in cases {
        let owner = unit_price_owner(&cart_source(new, ret))?;
        let old = (!old.is_empty()).then_some(old);
        assert_eq!(ts_lookup_table_change(&owner, new, old), None, "{label}");
    }
    // A table declared in an inner block leaves the `return` reading
    // another `table` (here a module-level one).
    let inner_block = format!(
        "const table = {{ apple: 120, pear: 95 }};\nexport function fetchUnitPrice(sku: string): number {{\n  {{\n{NEW}\n  }}\n  return table[sku] ?? 0;\n}}\n"
    );
    let owner = unit_price_owner(&inner_block)?;
    assert_eq!(ts_lookup_table_change(&owner, NEW, Some(OLD)), None);
    let arrow_inner_block = format!(
        "export const fetchUnitPrice = (sku: string): number => {{\n  {{\n{NEW}\n  }}\n  return table[sku] ?? 0;\n}};\n"
    );
    let owner = unit_price_owner(&arrow_inner_block)?;
    assert_eq!(ts_lookup_table_change(&owner, NEW, Some(OLD)), None);
    // An arrow owner with a block body reads as a function body does.
    let arrow = format!(
        "export const fetchUnitPrice = (sku: string): number => {{\n{NEW}\n  return table[sku] ?? 0;\n}};\n"
    );
    let owner = unit_price_owner(&arrow)?;
    assert_eq!(
        ts_lookup_table_change(&owner, NEW, Some(OLD)),
        Some(LookupTableChange {
            param_index: 0,
            changed_keys: vec!["pear".to_string()],
        })
    );
    Ok(())
}

fn unit_price_finding(label: &str, test: &str, ret: &str) -> Result<Finding, String> {
    lookup_finding(label, &cart_source(NEW, ret), test)
}

fn lookup_finding(label: &str, source: &str, test: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), source)?;
    ts_write_file(&root.join("test/cart.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/cart.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: NEW.to_string(),
            new_side_line: 2,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: OLD.to_string(),
            new_side_line: 2,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 2)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn vitest(import: &str, lines: &str) -> String {
    format!(
        "import {{ expect, it }} from \"vitest\";\n{import}\n\nit(\"looks up a price\", async () => {{\n{lines}\n}});\n"
    )
}

/// Rule 2: a strong assertion on a whole owner call with a changed key
/// credits the change, directly, through a rename or through a local.
#[test]
fn lookup_table_entry_is_exposed_when_a_test_looks_up_a_changed_key() -> Result<(), String> {
    let plain = "import { fetchUnitPrice } from \"../src/cart\";";
    for (label, import, lines, observed) in [
        (
            "lookup-resolves",
            plain,
            "  await expect(fetchUnitPrice(\"pear\")).resolves.toBe(95);",
            "fetchUnitPrice(\"pear\")",
        ),
        (
            "lookup-awaited-local",
            plain,
            "  const price = await fetchUnitPrice('pear');\n  expect(price).toBe(95);",
            "price",
        ),
        (
            "lookup-alias",
            "import { fetchUnitPrice as unit } from \"../src/cart\";",
            "  await expect(unit(\"pear\")).resolves.toBe(95);",
            "unit(\"pear\")",
        ),
        (
            "lookup-namespace",
            "import * as cart from \"../src/cart\";",
            "  await expect(cart.fetchUnitPrice(\"pear\")).resolves.toBe(95);",
            "cart.fetchUnitPrice(\"pear\")",
        ),
    ] {
        let test = vitest(import, lines);
        assert_eq!(
            extract_tests(Path::new("test/cart.test.ts"), &test)
                .iter()
                .flat_map(|test| test.assertions.iter())
                .map(|assertion| (
                    assertion.observed_expression.clone(),
                    assertion.oracle_kind.clone(),
                    assertion.oracle_strength.clone()
                ))
                .collect::<Vec<_>>(),
            vec![(
                Some(observed.to_string()),
                OracleKind::ExactValue,
                OracleStrength::Strong
            )],
            "{label}"
        );
        let finding = unit_price_finding(label, &test, "  return table[sku] ?? 0;")?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    // `toEqual` on the whole call, and a key that is the second argument.
    let finding = unit_price_finding(
        "lookup-to-equal",
        &vitest(
            plain,
            "  await expect(fetchUnitPrice(\"pear\")).resolves.toEqual(95);",
        ),
        "  return table[sku] ?? 0;",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let two_params = format!(
        "export function fetchUnitPrice(region: string, sku: string): number {{\n{NEW}\n  return table[sku] ?? 0;\n}}\n"
    );
    for (label, lines, class) in [
        (
            "lookup-second-argument",
            "  expect(fetchUnitPrice(\"eu\", \"pear\")).toBe(95);",
            ExposureClass::Exposed,
        ),
        (
            "lookup-key-in-first-argument",
            "  expect(fetchUnitPrice(\"pear\", \"apple\")).toBe(120);",
            ExposureClass::WeaklyExposed,
        ),
    ] {
        let finding = lookup_finding(label, &two_params, &vitest(plain, lines))?;
        assert_eq!(finding.class, class, "{label}: {:?}", finding.missing);
        // A miss still names the changed key, read from the right argument.
        assert_eq!(
            class == ExposureClass::WeaklyExposed,
            finding
                .missing
                .iter()
                .any(|line| line.contains("lookup-table value for `pear`")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// Rule 2 negatives: an exact assertion on an unchanged key, a key the test
/// computes, a member of the result, or a weak matcher keeps the change
/// `weakly_exposed`; each strong miss names the changed key.
#[test]
fn lookup_table_entry_stays_weak_without_a_changed_key_lookup() -> Result<(), String> {
    let plain = "import { fetchUnitPrice } from \"../src/cart\";";
    for (label, lines) in [
        (
            "lookup-other-key",
            "  await expect(fetchUnitPrice(\"apple\")).resolves.toBe(120);",
        ),
        (
            "lookup-variable-key",
            "  const sku = \"pear\";\n  await expect(fetchUnitPrice(sku)).resolves.toBe(95);",
        ),
        (
            "lookup-weak-matcher",
            "  await expect(fetchUnitPrice(\"pear\")).resolves.toBeGreaterThan(0);",
        ),
        (
            "lookup-template-key",
            "  await expect(fetchUnitPrice(`pear`)).resolves.toBe(95);",
        ),
        (
            "lookup-member-of-result",
            "  expect((await fetchUnitPrice(\"pear\")).toFixed(0)).toBe(\"95\");",
        ),
        (
            "lookup-self-compare",
            "  expect(await fetchUnitPrice(\"pear\")).toBe(await fetchUnitPrice(\"pear\"));",
        ),
        (
            "lookup-dynamic-expected",
            "  expect(await fetchUnitPrice(\"pear\")).toEqual(expect.any(Number));",
        ),
    ] {
        let finding =
            unit_price_finding(label, &vitest(plain, lines), "  return table[sku] ?? 0;")?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    // The inline snapshot on the changed key is strong but not exact, so
    // only the kind filter keeps it from crediting the change.
    let snapshot = vitest(
        plain,
        "  await expect(fetchUnitPrice(\"pear\")).resolves.toMatchInlineSnapshot(`95`);",
    );
    assert_eq!(
        extract_tests(Path::new("test/cart.test.ts"), &snapshot)
            .iter()
            .flat_map(|test| test.assertions.iter())
            .map(|assertion| (
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone()
            ))
            .collect::<Vec<_>>(),
        vec![(OracleKind::Snapshot, OracleStrength::Strong)]
    );
    // Every strong assertion that misses the changed key names it, and the
    // discriminate summary agrees.
    for (label, lines) in [
        (
            "lookup-other-key-message",
            "  await expect(fetchUnitPrice(\"apple\")).resolves.toBe(120);",
        ),
        (
            "lookup-variable-key-message",
            "  const sku = \"pear\";\n  await expect(fetchUnitPrice(sku)).resolves.toBe(95);",
        ),
        (
            "lookup-template-key-message",
            "  await expect(fetchUnitPrice(`pear`)).resolves.toBe(95);",
        ),
        (
            "lookup-member-of-result-message",
            "  expect((await fetchUnitPrice(\"pear\")).toFixed(0)).toBe(\"95\");",
        ),
        // A strong exact pin on an unchanged key beside a weak or a
        // non-exact strong assertion on the changed key.
        (
            "lookup-weak-on-changed-key",
            "  await expect(fetchUnitPrice(\"apple\")).resolves.toBe(120);\n  await expect(fetchUnitPrice(\"pear\")).resolves.toBeGreaterThan(0);",
        ),
        (
            "lookup-snapshot-on-changed-key",
            "  await expect(fetchUnitPrice(\"apple\")).resolves.toBe(120);\n  await expect(fetchUnitPrice(\"pear\")).resolves.toMatchInlineSnapshot(`95`);",
        ),
    ] {
        let finding =
            unit_price_finding(label, &vitest(plain, lines), "  return table[sku] ?? 0;")?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{label}: {:?}",
            finding.missing
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("lookup-table value for `pear`")),
            "{label}: {:?}",
            finding.missing
        );
        let summary = &finding.ripr.reveal.discriminate.summary;
        assert!(
            summary.contains("no strong assertion looks up a changed key")
                && summary.contains("`pear`"),
            "{label}: {summary}"
        );
    }
    Ok(())
}

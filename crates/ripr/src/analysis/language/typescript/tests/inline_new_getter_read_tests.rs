//! #16353: a getter read straight off a fresh instance
//! (`expect(new Cart(100).total).toBe(95)`) runs the getter body, like a read
//! through a named receiver (`const cart = new Cart(100); cart.total`). Before,
//! only the receiver read and an inline method call (`new Cart().m(`) were
//! needles, so the inline read reported `no_static_path`.

use super::*;

const CART: &str = "export class Cart {\n  constructor(private n: number) {}\n  get total(): number {\n    if (this.n > 50) {\n      return this.n - 5;\n    }\n    return this.n;\n  }\n  get totalPrice(): number {\n    return this.n;\n  }\n}\nexport class Other {\n  constructor(private n: number) {}\n  get total(): number {\n    return this.n;\n  }\n}\n";

const CHANGED: &str = "    if (this.n > 50) {";

/// A jest test file whose `test` body is `body`, importing both classes.
fn test_file(body: &str) -> String {
    format!(
        "import {{ Cart, Other }} from '../src/cart';\n\ndescribe('Cart', () => {{\n  test('total', () => {{\n{body}\n  }});\n}});\n"
    )
}

/// The finding for the changed line of `Cart`'s `total` getter in `source`.
fn total_finding(label: &str, source: &str, test_source: &str) -> Result<Finding, String> {
    let line = source
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), source)?;
    ts_write_file(&root.join("tests/cart.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.ts", &[(line, CHANGED)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let finding = result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::total"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for the `total` getter"))?;
    // The fixture really is the getter at the changed line, not `Other`'s.
    assert!(
        finding.evidence.iter().any(|line| line == "owner: total"),
        "{label}: {:?}",
        finding.evidence
    );
    Ok(finding)
}

fn assert_related(label: &str, body: &str) -> Result<(), String> {
    let finding = total_finding(label, CART, &test_file(body))?;
    assert!(
        !finding.related_tests.is_empty(),
        "{label}: the read runs the getter"
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    assert_ne!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

fn assert_unrelated(label: &str, body: &str) -> Result<(), String> {
    let finding = total_finding(label, CART, &test_file(body))?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

#[test]
fn a_getter_read_on_a_fresh_instance_relates() -> Result<(), String> {
    for (label, body) in [
        (
            "inline-getter-read",
            "    expect(new Cart(100).total).toBe(95);",
        ),
        (
            "inline-getter-optional",
            "    expect(new Cart(100)?.total).toBe(95);",
        ),
        (
            "inline-getter-wrapped",
            "    expect(\n      new Cart(100)\n        .total\n    ).toBe(95);",
        ),
        (
            "inline-getter-chained",
            "    expect(new Cart(100).total.toFixed(0)).toBe('95');",
        ),
        (
            "inline-getter-receiver-control",
            "    const cart = new Cart(100);\n    expect(cart.total).toBe(95);",
        ),
    ] {
        assert_related(label, body)?;
    }
    Ok(())
}

#[test]
fn other_reads_off_a_fresh_instance_stay_unrelated() -> Result<(), String> {
    for (label, body) in [
        // A longer property is a different getter.
        (
            "inline-longer-name",
            "    expect(new Cart(50).totalPrice).toBe(50);",
        ),
        // Another class's getter of the same name.
        (
            "inline-other-class",
            "    expect(new Other(50).total).toBe(50);",
        ),
        // Text that only looks like the read.
        (
            "inline-in-comment",
            "    // expect(new Cart(50).total).toBe(45);\n    expect(new Other(1).total).toBe(1);",
        ),
        (
            "inline-in-string",
            "    expect('new Cart(50).total').toContain('Cart');",
        ),
        // A spied getter is not the getter as written.
        (
            "inline-spied-getter",
            "    jest.spyOn(Cart.prototype, 'total', 'get').mockReturnValue(45);\n    expect(new Cart(50).total).toBe(45);",
        ),
        // A `delete` of the member does not run the getter, through a fresh
        // instance or a receiver (review of #16360). The literal inline form
        // is the one the inline read matcher sees, so only the delete guard
        // keeps it unrelated; the cast form never reaches that matcher.
        (
            "inline-deleted",
            "    // @ts-expect-error the getter is not optional\n    expect(delete new Cart(100).total).toBe(true);",
        ),
        (
            "inline-deleted-cast",
            "    expect(delete (new Cart(100) as any).total).toBe(true);",
        ),
        (
            "receiver-deleted",
            "    const cart: any = new Cart(100);\n    expect(delete cart.total).toBe(true);",
        ),
        // An assignment does not read the getter.
        (
            "inline-assigned",
            "    expect(() => {\n      new Cart(50).total = 3;\n    }).toThrow();",
        ),
    ] {
        assert_unrelated(label, body)?;
    }
    Ok(())
}

#[test]
fn a_bare_read_of_an_ordinary_method_stays_unrelated() -> Result<(), String> {
    // An ordinary method runs only when called; reading it off a fresh
    // instance does not execute its body.
    let source = CART.replace("  get total(): number {", "  total(): number {");
    let finding = total_finding(
        "inline-method-read",
        &source,
        &test_file("    expect(typeof new Cart(50).total).toBe('function');"),
    )?;
    assert!(
        finding.related_tests.is_empty(),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    Ok(())
}

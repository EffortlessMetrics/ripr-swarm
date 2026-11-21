//! Method owners called on a receiver that is not a literal `new Owner()` of
//! the declaring class (#15924):
//!
//! - `const c = makeCart()`: an exported factory of the owner's module whose
//!   every return is `new Cart(...)`;
//! - `const c = Cart.make()`: a static factory of the owner's class;
//! - `const c = new SpecialCart()`: a class that inherits the method
//!   unchanged, in the owner's file or another module.
//!
//! Every case runs `TypeScriptAdapter::analyze_diff` over a real workspace.
//! A positive row must read exactly as the `new Cart()` form of the same
//! test does (same class, same relation), so the new receivers can lend
//! nothing a constructed receiver does not. Every shape that could make the
//! receiver or the method something else must stay `no_static_path`.

use super::*;

const CHANGED: &str = "        if (amount > threshold) {";

fn class_source(name: &str, extra_members: &str) -> String {
    format!(
        "export class {name} {{\n    total = 0;\n    apply(amount: number, threshold: number): number {{\n        if (amount > threshold) {{\n            return amount - 10;\n        }}\n        return amount;\n    }}\n{extra_members}}}\n"
    )
}

/// `Cart` with a static factory, plus `tail` after the class.
fn cart(tail: &str) -> String {
    format!(
        "{}{tail}",
        class_source("Cart", "    static make(): Cart { return new Cart(); }\n")
    )
}

fn base_cart() -> String {
    class_source("BaseCart", "")
}

/// A vitest file with `imports`, file-level `setup` and one test that runs
/// `receiver` and then asserts `c.apply(100, 100)` is `100`.
fn test_file(imports: &str, setup: &str, receiver: &str) -> String {
    format!(
        "import {{ describe, it, expect, vi, beforeEach }} from 'vitest';\n{imports}\n{setup}\ndescribe('cart', () => {{\n  it('keeps amount at threshold', () => {{\n    {receiver}\n    expect(c.apply(100, 100)).toBe(100);\n  }});\n}});\n"
    )
}

/// The finding for the changed `apply` boundary in `src/cart.ts`.
fn finding(label: &str, files: &[(&str, &str)], test: &str) -> Result<Finding, String> {
    let owner = files
        .iter()
        .find(|(path, _)| *path == "src/cart.ts")
        .map(|(_, source)| *source)
        .ok_or_else(|| format!("{label}: the fixture has no src/cart.ts"))?;
    let line = owner
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    for (path, source) in files {
        ts_write_file(&root.join(path), source)?;
    }
    ts_write_file(&root.join("test/c.test.ts"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.ts", &[(line, CHANGED)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::apply"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `apply`"))
}

/// What the test's `new <Class>()` form reads for the same change: the
/// class and the related tests' relation reasons.
type Reading = (ExposureClass, Vec<Option<crate::domain::RelationReason>>);

fn reading(finding: &Finding) -> Reading {
    (
        finding.class.clone(),
        finding
            .related_tests
            .iter()
            .map(|related| related.relation_reason)
            .collect(),
    )
}

/// The `new Cart()` reading every positive row must equal.
fn constructed_reading() -> Result<Reading, String> {
    let owner = cart("");
    let finding = finding(
        "recv-baseline",
        &[("src/cart.ts", &owner)],
        &test_file(
            "import { Cart } from '../src/cart';",
            "",
            "const c = new Cart();",
        ),
    )?;
    let reading = reading(&finding);
    assert_eq!(
        reading.1.len(),
        1,
        "baseline: the constructed receiver relates the test"
    );
    assert_ne!(reading.0, ExposureClass::NoStaticPath, "baseline");
    Ok(reading)
}

fn assert_reads_like_constructed(
    label: &str,
    files: &[(&str, &str)],
    test: &str,
) -> Result<(), String> {
    let expected = constructed_reading()?;
    let finding = finding(label, files, test)?;
    assert_eq!(reading(&finding), expected, "{label}");
    Ok(())
}

fn assert_no_static_path(label: &str, files: &[(&str, &str)], test: &str) -> Result<(), String> {
    let finding = finding(label, files, test)?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: no path is established, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

// ── factories ───────────────────────────────────────────────────────────────

const FUNCTION_FACTORY: &str = "export function makeCart(): Cart { return new Cart(); }\n";

#[test]
fn exported_factory_receiver_reads_like_a_constructed_receiver() -> Result<(), String> {
    let rows = [
        (
            "recv-function-factory",
            FUNCTION_FACTORY,
            "import { makeCart } from '../src/cart';",
            "const c = makeCart();",
        ),
        (
            "recv-arrow-factory",
            "export const makeCart = (): Cart => new Cart();\n",
            "import { makeCart } from '../src/cart';",
            "const c = makeCart();",
        ),
        (
            "recv-renamed-factory-import",
            FUNCTION_FACTORY,
            "import { makeCart as build } from '../src/cart';",
            "const c = build();",
        ),
        (
            "recv-namespace-factory",
            FUNCTION_FACTORY,
            "import * as shop from '../src/cart';",
            "const c = shop.makeCart();",
        ),
    ];
    for (label, tail, imports, receiver) in rows {
        let owner = cart(tail);
        assert_reads_like_constructed(
            label,
            &[("src/cart.ts", &owner)],
            &test_file(imports, "", receiver),
        )?;
    }
    Ok(())
}

#[test]
fn static_factory_receiver_reads_like_a_constructed_receiver() -> Result<(), String> {
    let rows = [
        (
            "recv-static-factory",
            "import { Cart } from '../src/cart';",
            "const c = Cart.make();",
        ),
        (
            "recv-static-factory-alias",
            "import { Cart as Basket } from '../src/cart';",
            "const c = Basket.make();",
        ),
        (
            "recv-static-factory-namespace",
            "import * as shop from '../src/cart';",
            "const c = shop.Cart.make();",
        ),
    ];
    let owner = cart("");
    for (label, imports, receiver) in rows {
        assert_reads_like_constructed(
            label,
            &[("src/cart.ts", &owner)],
            &test_file(imports, "", receiver),
        )?;
    }
    Ok(())
}

#[test]
fn factory_that_may_return_something_else_stays_unreached() -> Result<(), String> {
    let rows = [
        (
            "recv-neg-factory-cached-return",
            "let cached: Cart | undefined;\nexport function makeCart(): Cart {\n  if (cached) { return cached; }\n  return new Cart();\n}\n",
        ),
        (
            "recv-neg-factory-async",
            "export async function makeCart(): Promise<Cart> { return new Cart(); }\n",
        ),
        (
            "recv-neg-factory-other-class",
            "class Other { apply(amount: number, threshold: number): number { return amount; } }\nexport function makeCart(): Other { return new Other(); }\n",
        ),
        (
            "recv-neg-factory-wrapped",
            "export function makeCart(): Cart { return wrap(new Cart()); }\nfunction wrap(cart: Cart): Cart { return { apply: () => 100 } as unknown as Cart; }\n",
        ),
        (
            "recv-neg-factory-reassigned",
            "export let makeCart = (): Cart => new Cart();\nmakeCart = () => ({ apply: () => 100 }) as unknown as Cart;\n",
        ),
    ];
    for (label, tail) in rows {
        let owner = cart(tail);
        assert_no_static_path(
            label,
            &[("src/cart.ts", &owner)],
            &test_file(
                "import { makeCart } from '../src/cart';",
                "",
                "const c = makeCart();",
            ),
        )?;
    }
    Ok(())
}

#[test]
fn static_member_that_is_not_a_sole_fresh_instance_factory_stays_unreached() -> Result<(), String> {
    let rows = [
        (
            "recv-neg-static-new-this",
            "    static make(): Cart { return new this(); }\n",
            "",
        ),
        (
            "recv-neg-static-cached",
            "    static shared = new Cart();\n    static make(): Cart { return Cart.shared; }\n",
            "",
        ),
        (
            "recv-neg-static-reassigned-in-owner",
            "    static make(): Cart { return new Cart(); }\n",
            "Cart.make = () => ({ apply: () => 100 }) as unknown as Cart;\n",
        ),
        (
            "recv-neg-static-async",
            "    static async make(): Promise<Cart> { return new Cart(); }\n",
            "",
        ),
    ];
    for (label, members, tail) in rows {
        let owner = format!("{}{tail}", class_source("Cart", members));
        assert_no_static_path(
            label,
            &[("src/cart.ts", &owner)],
            &test_file(
                "import { Cart } from '../src/cart';",
                "",
                "const c = Cart.make();",
            ),
        )?;
    }
    Ok(())
}

/// What the test file does can make the receiver, the factory or the method
/// something else; each withholds the factory reach.
#[test]
fn factory_receiver_the_test_can_replace_stays_unreached() -> Result<(), String> {
    let function_import = "import { makeCart } from '../src/cart';";
    let class_import = "import { Cart } from '../src/cart';";
    let rows = [
        (
            "recv-neg-test-shadows-factory",
            function_import,
            "",
            "const makeCart = () => ({ apply: (a: number, t: number) => a });\n    const c = makeCart();",
        ),
        (
            "recv-neg-test-reassigns-receiver",
            function_import,
            "",
            "let c = makeCart();\n    c = { apply: (a: number, t: number) => a } as any;",
        ),
        (
            "recv-neg-test-assigns-method",
            function_import,
            "",
            "const c = makeCart();\n    c.apply = () => 100;",
        ),
        (
            "recv-neg-test-spies-prototype",
            "import { Cart, makeCart } from '../src/cart';",
            "beforeEach(() => { vi.spyOn(Cart.prototype, 'apply').mockReturnValue(100); });",
            "const c = makeCart();",
        ),
        (
            "recv-neg-test-mocks-owner-module",
            function_import,
            "vi.mock('../src/cart');",
            "const c = makeCart();",
        ),
        (
            "recv-neg-test-replaces-static",
            class_import,
            "beforeEach(() => { Cart.make = () => ({ apply: () => 100 }) as any; });",
            "const c = Cart.make();",
        ),
        (
            "recv-neg-test-spies-static",
            class_import,
            "beforeEach(() => { vi.spyOn(Cart, 'make').mockReturnValue({ apply: () => 100 } as any); });",
            "const c = Cart.make();",
        ),
        (
            "recv-neg-test-member-of-factory-result",
            function_import,
            "",
            "const c = makeCart().clone();",
        ),
    ];
    let owner = cart(FUNCTION_FACTORY);
    for (label, imports, setup, receiver) in rows {
        assert_no_static_path(
            label,
            &[("src/cart.ts", &owner)],
            &test_file(imports, setup, receiver),
        )?;
    }
    Ok(())
}

// ── inherited members ───────────────────────────────────────────────────────

const SPECIAL: &str = "import { BaseCart } from './cart';\nexport class SpecialCart extends BaseCart { extra() { return 1; } }\n";

fn special_test(setup: &str) -> String {
    test_file(
        "import { SpecialCart } from '../src/special';",
        setup,
        "const c = new SpecialCart();",
    )
}

#[test]
fn inherited_method_on_a_subclass_instance_reads_like_a_constructed_receiver() -> Result<(), String>
{
    let base = base_cart();
    let same_file =
        format!("{base}export class SpecialCart extends BaseCart {{ extra() {{ return 1; }} }}\n");
    let rows: [(&str, Vec<(&str, &str)>, String); 5] = [
        (
            "recv-inherited-other-module",
            vec![("src/cart.ts", &base), ("src/special.ts", SPECIAL)],
            special_test(""),
        ),
        (
            "recv-inherited-same-file",
            vec![("src/cart.ts", &same_file)],
            test_file(
                "import { SpecialCart } from '../src/cart';",
                "",
                "const c = new SpecialCart();",
            ),
        ),
        (
            "recv-inherited-two-levels",
            vec![
                ("src/cart.ts", &base),
                (
                    "src/mid.ts",
                    "import { BaseCart } from './cart';\nexport class MidCart extends BaseCart { mid() { return 1; } }\n",
                ),
                (
                    "src/special.ts",
                    "import { MidCart } from './mid';\nexport class SpecialCart extends MidCart { extra() { return 1; } }\n",
                ),
            ],
            special_test(""),
        ),
        (
            "recv-inherited-default-export-subclass",
            vec![
                ("src/cart.ts", &base),
                (
                    "src/special.ts",
                    "import { BaseCart } from './cart';\nexport default class SpecialCart extends BaseCart { extra() { return 1; } }\n",
                ),
            ],
            test_file(
                "import Special from '../src/special';",
                "",
                "const c = new Special();",
            ),
        ),
        (
            "recv-inherited-base-through-barrel",
            vec![
                ("src/cart.ts", &base),
                ("src/index.ts", "export * from './cart';\n"),
                (
                    "src/special.ts",
                    "import { BaseCart as Parent } from './index';\nexport class SpecialCart extends Parent {\n  #count = 0;\n  constructor() { super(); this.total = 1; }\n}\n",
                ),
            ],
            special_test(""),
        ),
    ];
    for (label, files, test) in rows {
        assert_reads_like_constructed(label, &files, &test)?;
    }
    Ok(())
}

/// The override control and every way a class on the chain can supply its
/// own `apply`.
#[test]
fn subclass_that_can_supply_its_own_member_stays_unreached() -> Result<(), String> {
    let import = "import { BaseCart } from './cart';\n";
    let rows = [
        (
            "recv-neg-override-method",
            "export class SpecialCart extends BaseCart { apply(a: number, t: number) { return a; } }\n",
        ),
        (
            "recv-neg-override-field",
            "export class SpecialCart extends BaseCart { apply = (a: number, t: number) => a; }\n",
        ),
        (
            "recv-neg-override-getter",
            "export class SpecialCart extends BaseCart { get apply() { return (a: number, t: number) => a; } }\n",
        ),
        (
            "recv-neg-constructor-assigns",
            "export class SpecialCart extends BaseCart { constructor() { super(); this.apply = (a: number, t: number) => a; } }\n",
        ),
        (
            "recv-neg-parameter-property",
            "export class SpecialCart extends BaseCart { constructor(public apply: any = (a: number, t: number) => a) { super(); } }\n",
        ),
        (
            "recv-neg-computed-member",
            "const key = 'apply';\nexport class SpecialCart extends BaseCart { [key](a: number, t: number) { return a; } }\n",
        ),
        (
            "recv-neg-string-key-member",
            "export class SpecialCart extends BaseCart { 'apply'(a: number, t: number) { return a; } }\n",
        ),
        (
            "recv-neg-index-assign",
            "export class SpecialCart extends BaseCart { constructor() { super(); this['apply'] = (a: number, t: number) => a; } }\n",
        ),
        (
            "recv-neg-static-block",
            "export class SpecialCart extends BaseCart { static { SpecialCart.prototype.apply = (a: number, t: number) => a; } }\n",
        ),
        (
            "recv-neg-prototype-edit",
            "export class SpecialCart extends BaseCart { extra() { return 1; } }\nSpecialCart.prototype.apply = (a: number, t: number) => a;\n",
        ),
        (
            "recv-neg-define-property",
            "export class SpecialCart extends BaseCart { constructor() { super(); Object.defineProperty(this, 'apply', { value: () => 100 }); } }\n",
        ),
        (
            "recv-neg-decorated",
            "function swap(target: any): any { return class { apply() { return 100; } }; }\n@swap\nexport class SpecialCart extends BaseCart { extra() { return 1; } }\n",
        ),
        (
            "recv-neg-constructor-returns",
            "export class SpecialCart extends BaseCart { constructor() { super(); return { apply: () => 100 } as any; } }\n",
        ),
        (
            "recv-neg-mixin-heritage",
            "function mix(base: any): any { return class extends base { apply() { return 100; } }; }\nexport class SpecialCart extends mix(BaseCart) { extra() { return 1; } }\n",
        ),
        (
            "recv-neg-reassigned-class",
            "export let SpecialCart = class extends BaseCart {};\nSpecialCart = class extends BaseCart { apply() { return 100; } } as any;\n",
        ),
    ];
    let base = base_cart();
    for (label, special) in rows {
        let special = format!("{import}{special}");
        assert_no_static_path(
            label,
            &[("src/cart.ts", &base), ("src/special.ts", &special)],
            &special_test(""),
        )?;
    }
    Ok(())
}

#[test]
fn subclass_chain_that_does_not_bind_the_owner_class_stays_unreached() -> Result<(), String> {
    let base = base_cart();
    let other_base = class_source("BaseCart", "").replace("amount - 10", "amount - 20");
    let rows: [(&str, Vec<(&str, &str)>); 4] = [
        // A same-named class of another module.
        (
            "recv-neg-other-same-named-base",
            vec![
                ("src/cart.ts", &base),
                ("src/other.ts", &other_base),
                (
                    "src/special.ts",
                    "import { BaseCart } from './other';\nexport class SpecialCart extends BaseCart { extra() { return 1; } }\n",
                ),
            ],
        ),
        // The override sits on a class between the owner and the subclass.
        (
            "recv-neg-intermediate-override",
            vec![
                ("src/cart.ts", &base),
                (
                    "src/mid.ts",
                    "import { BaseCart } from './cart';\nexport class MidCart extends BaseCart { apply(a: number, t: number) { return a; } }\n",
                ),
                (
                    "src/special.ts",
                    "import { MidCart } from './mid';\nexport class SpecialCart extends MidCart { extra() { return 1; } }\n",
                ),
            ],
        ),
        // A local class of the subclass's own file shadows the import-free name.
        (
            "recv-neg-local-same-named-base",
            vec![
                ("src/cart.ts", &base),
                (
                    "src/special.ts",
                    "class BaseCart { apply(a: number, t: number) { return a; } }\nexport class SpecialCart extends BaseCart { extra() { return 1; } }\n",
                ),
            ],
        ),
        // Four classes below the owner is past the chain bound.
        (
            "recv-neg-chain-past-bound",
            vec![
                ("src/cart.ts", &base),
                (
                    "src/a.ts",
                    "import { BaseCart } from './cart';\nexport class A extends BaseCart {}\n",
                ),
                (
                    "src/b.ts",
                    "import { A } from './a';\nexport class B extends A {}\n",
                ),
                (
                    "src/c.ts",
                    "import { B } from './b';\nexport class C extends B {}\n",
                ),
                (
                    "src/special.ts",
                    "import { C } from './c';\nexport class SpecialCart extends C {}\n",
                ),
            ],
        ),
    ];
    for (label, files) in rows {
        assert_no_static_path(label, &files, &special_test(""))?;
    }
    Ok(())
}

#[test]
fn subclass_receiver_the_test_can_replace_stays_unreached() -> Result<(), String> {
    let base = base_cart();
    let files = [("src/cart.ts", base.as_str()), ("src/special.ts", SPECIAL)];
    let rows = [
        (
            "recv-neg-mock-subclass-module",
            "vi.mock('../src/special');",
        ),
        ("recv-neg-mock-base-module", "vi.mock('../src/cart');"),
        (
            "recv-neg-spy-subclass-prototype",
            "beforeEach(() => { vi.spyOn(SpecialCart.prototype, 'apply').mockReturnValue(100); });",
        ),
        (
            "recv-neg-assign-subclass-prototype",
            "beforeEach(() => { SpecialCart.prototype.apply = () => 100; });",
        ),
    ];
    for (label, setup) in rows {
        assert_no_static_path(label, &files, &special_test(setup))?;
    }
    // A body-local class of the subclass's name constructs that class.
    assert_no_static_path(
        "recv-neg-body-local-subclass",
        &files,
        &test_file(
            "import { SpecialCart as Imported } from '../src/special';",
            "",
            "class SpecialCart { apply(a: number, t: number) { return a; } }\n    void Imported;\n    const c = new SpecialCart();",
        ),
    )
}

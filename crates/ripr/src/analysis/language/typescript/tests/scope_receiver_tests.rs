//! Method-owner relations through receivers built outside the test body: in
//! an enclosing `describe`, in a `beforeEach`/`beforeAll` hook, or through a
//! default or namespace import of the owner's class. Each of these reached the
//! method at runtime but was reported `no_static_path` (2026-09-27 re-walk).

use super::*;

const CART: &str = "export class Cart {\n  private items: number[] = [];\n\n  add(qty: number): void {\n    this.items.push(qty);\n  }\n\n  total(): number {\n    return this.items.reduce((a, b) => a + b, 1);\n  }\n}\n";
const TOTAL_LINE: (usize, &str) = (9, "    return this.items.reduce((a, b) => a + b, 1);");

fn total_finding(label: &str, owner_source: &str, test_source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), owner_source)?;
    ts_write_file(&root.join("tests/cart.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.ts", &[TOTAL_LINE])],
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
                .is_some_and(|owner| owner.0.ends_with("total"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `total`"))
}

fn assert_related(label: &str, test_source: &str) -> Result<(), String> {
    let finding = total_finding(label, CART, test_source)?;
    assert_eq!(
        finding.ripr.reach.state,
        StageState::Yes,
        "{label}: the test calls `total` on a Cart, evidence: {:?}",
        finding.evidence
    );
    assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    Ok(())
}

fn assert_unrelated(label: &str, test_source: &str) -> Result<(), String> {
    let finding = total_finding(label, CART, test_source)?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{label}: the receiver is not a Cart, evidence: {:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn receiver_assigned_in_before_each_relates() -> Result<(), String> {
    assert_related(
        "before-each",
        "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  let cart: Cart;\n  beforeEach(() => {\n    cart = new Cart();\n  });\n\n  it('totals', () => {\n    cart.add(2);\n    expect(cart.total()).toBe(3);\n  });\n});\n",
    )
}

#[test]
fn receiver_declared_in_enclosing_describe_relates() -> Result<(), String> {
    assert_related(
        "describe-const",
        "import { describe, it, expect } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  const cart = new Cart();\n  describe('empty', () => {\n    it('totals', () => {\n      expect(cart.total()).toBe(1);\n    });\n  });\n});\n",
    )
}

#[test]
fn receiver_built_through_namespace_import_relates() -> Result<(), String> {
    assert_related(
        "namespace",
        "import { it, expect } from 'vitest';\nimport * as shop from '../src/cart';\n\nit('totals', () => {\n  const cart = new shop.Cart();\n  expect(cart.total()).toBe(1);\n});\n",
    )
}

#[test]
fn receiver_built_through_default_import_relates() -> Result<(), String> {
    let owner = CART.replacen("export class Cart", "export default class Cart", 1);
    let finding = total_finding(
        "default-import",
        &owner,
        "import { it, expect } from 'vitest';\nimport Basket from '../src/cart';\n\nit('totals', () => {\n  const cart = new Basket();\n  expect(cart.total()).toBe(1);\n});\n",
    )?;
    assert_eq!(
        finding.ripr.reach.state,
        StageState::Yes,
        "{:?}",
        finding.evidence
    );
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn default_import_of_a_named_class_does_not_relate() -> Result<(), String> {
    // `Cart` is a named export; the default import is some other value.
    assert_unrelated(
        "default-import-named-class",
        "import { it, expect } from 'vitest';\nimport Basket from '../src/cart';\n\nit('totals', () => {\n  const cart = new Basket();\n  expect(cart.total()).toBe(1);\n});\n",
    )
}

#[test]
fn body_local_shadows_scope_receiver() -> Result<(), String> {
    assert_unrelated(
        "body-shadow",
        "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  let cart: Cart;\n  beforeEach(() => {\n    cart = new Cart();\n  });\n\n  it('totals', () => {\n    const cart = { total: () => 1 };\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn sibling_describe_setup_does_not_leak() -> Result<(), String> {
    assert_unrelated(
        "sibling-describe",
        "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\n\nconst cart = { total: () => 1 };\n\ndescribe('a', () => {\n  let other: Cart;\n  beforeEach(() => {\n    other = new Cart();\n  });\n});\n\ndescribe('b', () => {\n  it('totals', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn after_each_hook_is_not_setup() -> Result<(), String> {
    assert_unrelated(
        "after-each",
        "import { describe, it, expect, afterEach } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  let cart: Cart;\n  afterEach(() => {\n    cart = new Cart();\n  });\n\n  it('totals', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn skipped_test_is_not_setup() -> Result<(), String> {
    assert_unrelated(
        "skipped-test",
        "import { describe, it, expect } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  let cart: Cart;\n  it.skip('skipped', () => {\n    cart = new Cart();\n  });\n\n  it('totals', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn scope_receiver_also_assigned_something_else_is_ambiguous() -> Result<(), String> {
    assert_unrelated(
        "reassigned",
        "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\n\ndescribe('Cart', () => {\n  let cart: any;\n  beforeEach(() => {\n    cart = new Cart();\n    cart = { total: () => 1 };\n  });\n\n  it('totals', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

// ── Adversarial shapes (review of the first candidate): each binds the
// receiver or the constructor name to something other than a Cart. ──────────

const HEAD: &str = "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\nimport * as shop from '../src/cart';\n";

fn unrelated_with_head(label: &str, rest: &str) -> Result<(), String> {
    assert_unrelated(label, &format!("{HEAD}{rest}"))
}

#[test]
fn body_reassigns_scope_receiver() -> Result<(), String> {
    unrelated_with_head(
        "adv-body-reassign",
        "describe('C', () => {\n  let cart: any;\n  beforeEach(() => {\n    cart = new Cart();\n  });\n  it('t', () => {\n    cart = { total: () => 1 };\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn nested_destructuring_shadows_outer_receiver() -> Result<(), String> {
    unrelated_with_head(
        "adv-destructure",
        "const fakes = { cart: { total: () => 1 } };\ndescribe('C', () => {\n  const cart = new Cart();\n  describe('inner', () => {\n    const { cart } = fakes;\n    it('t', () => {\n      expect(cart.total()).toBe(1);\n    });\n  });\n});\n",
    )
}

#[test]
fn test_each_parameter_shadows_scope_receiver() -> Result<(), String> {
    unrelated_with_head(
        "adv-it-each",
        "describe('C', () => {\n  const cart = new Cart();\n  it.each([[{ total: () => 1 }]])('t %s', (cart) => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn describe_each_parameter_shadows_file_receiver() -> Result<(), String> {
    unrelated_with_head(
        "adv-describe-each",
        "const cart = new Cart();\ndescribe.each([[{ total: () => 1 }]])('C %s', (cart) => {\n  it('t', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn inner_single_line_async_hook_shadows_outer_receiver() -> Result<(), String> {
    unrelated_with_head(
        "adv-async-hook",
        "const makeFake = async () => ({ total: () => 1 });\ndescribe('C', () => {\n  const cart = new Cart();\n  describe('inner', () => {\n    let cart: any;\n    beforeEach(async () => { cart = await makeFake(); });\n    it('t', () => {\n      expect(cart.total()).toBe(1);\n    });\n  });\n});\n",
    )
}

#[test]
fn second_assignment_on_one_line_is_ambiguous() -> Result<(), String> {
    unrelated_with_head(
        "adv-one-line",
        "describe('C', () => {\n  let cart: any;\n  beforeEach(() => { cart = new Cart(); cart = { total: () => 1 }; });\n  it('t', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn conditional_hook_assignment_is_ambiguous() -> Result<(), String> {
    unrelated_with_head(
        "adv-conditional",
        "describe('C', () => {\n  let cart: any;\n  beforeEach(() => {\n    cart = new Cart();\n    if (process.env.FAKE) {\n      cart = { total: () => 1 };\n    }\n  });\n  it('t', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn hook_local_declaration_does_not_leak() -> Result<(), String> {
    assert_unrelated(
        "adv-hook-local",
        "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\nimport { cart } from './fixtures';\n\ndescribe('C', () => {\n  beforeEach(() => {\n    const cart = new Cart();\n    cart.add(1);\n  });\n  it('t', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn helper_local_template_and_comment_do_not_bind() -> Result<(), String> {
    assert_unrelated(
        "adv-text-only",
        "import { describe, it, expect, beforeEach } from 'vitest';\nimport { Cart } from '../src/cart';\nimport { cart } from './fixtures';\n\nconst make = () => {\n  const cart = new Cart();\n  return cart;\n};\nconst snippet = `\n  cart = new Cart();\n`;\ndescribe('C', () => {\n  beforeEach(() => {\n    /*\n    cart = new Cart();\n    */\n  });\n  it('t', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn scope_class_shadow_constructs_something_else() -> Result<(), String> {
    unrelated_with_head(
        "adv-class-shadow",
        "describe('C', () => {\n  const Cart = class { total() { return 1; } };\n  const cart = new Cart();\n  it('t', () => {\n    expect(cart.total()).toBe(1);\n  });\n});\n",
    )
}

#[test]
fn body_namespace_shadow_constructs_something_else() -> Result<(), String> {
    unrelated_with_head(
        "adv-ns-shadow",
        "it('t', () => {\n  const shop = { Cart: class { total() { return 1; } } };\n  const cart = new shop.Cart();\n  expect(cart.total()).toBe(1);\n});\n",
    )
}

#[test]
fn namespace_member_of_default_exported_class_does_not_relate() -> Result<(), String> {
    let owner = CART.replacen("export class Cart", "export default class Cart", 1);
    let finding = total_finding(
        "adv-ns-default",
        &owner,
        "import { it, expect } from 'vitest';\nimport * as shop from '../src/cart';\n\nit('t', () => {\n  const other = new shop.Cart();\n  expect(other.total()).toBe(1);\n});\n",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn multi_binding_declaration_credits_only_the_constructed_name() -> Result<(), String> {
    unrelated_with_head(
        "adv-multi-binding",
        "describe('C', () => {\n  let fake = { total: () => 1 }, real = new Cart();\n  it('t', () => {\n    expect(fake.total()).toBe(1);\n  });\n});\n",
    )?;
    assert_related(
        "multi-binding-real",
        &format!(
            "{HEAD}describe('C', () => {{\n  let fake = {{ total: () => 1 }}, real = new Cart();\n  it('t', () => {{\n    expect(real.total()).toBe(1);\n  }});\n}});\n"
        ),
    )
}

#[test]
fn single_line_hook_assignment_relates() -> Result<(), String> {
    assert_related(
        "single-line-hook",
        &format!(
            "{HEAD}describe('C', () => {{\n  let cart: Cart;\n  beforeEach(() => (cart = new Cart()));\n  it('t', () => {{\n    expect(cart.total()).toBe(1);\n  }});\n}});\n"
        ),
    )
}

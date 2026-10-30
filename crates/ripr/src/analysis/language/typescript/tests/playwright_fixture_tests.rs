//! RIPR-SPEC-0243 rule 23: a destructured Playwright fixture that constructs
//! a class binds its parameter name to that class, and nothing wider does.

use super::super::tests_extract::extract_tests;
use std::path::Path;

fn spec(fixtures: &str, tests: &str) -> String {
    format!(
        "import {{ expect, test as base }} from \"@playwright/test\";\nimport {{ Cart }} from \"../src/cart\";\n{fixtures}\n{tests}\n"
    )
}

const PLAIN: &str = "const test = base.extend<{ cart: Cart }>({\n  cart: async ({}, use) => {\n    await use(new Cart());\n  },\n});";
const USES: &str =
    "test(\"adds\", ({ cart }) => {\n  cart.add(250);\n  expect(cart.total()).toBe(250);\n});";

/// `(test title, constructor)` for each binding named `name` the file's
/// tests carry.
fn constructors(source: &str, name: &str) -> Vec<Option<String>> {
    extract_tests(Path::new("test/cart.spec.ts"), source)
        .iter()
        .map(|test| {
            test.scope_bindings
                .iter()
                .find(|binding| binding.name == name)
                .and_then(|binding| binding.constructed_by.clone())
        })
        .collect()
}

#[test]
fn a_fixture_that_constructs_a_class_binds_the_parameter_to_it() {
    assert_eq!(
        constructors(&spec(PLAIN, USES), "cart"),
        vec![Some("Cart".to_string())]
    );
}

#[test]
fn a_renamed_destructuring_binds_the_local_name() {
    let tests = "test(\"adds\", ({ cart: basket }) => {\n  basket.add(250);\n});";
    assert_eq!(
        constructors(&spec(PLAIN, tests), "basket"),
        vec![Some("Cart".to_string())]
    );
}

#[test]
fn a_fixture_that_does_more_than_construct_binds_nothing() {
    for (label, fixture) in [
        (
            "a second statement seeds the instance",
            "cart: async ({}, use) => {\n    const made = new Cart();\n    made.add(1);\n    await use(made);\n  },",
        ),
        (
            "a statement before the construction",
            "cart: async ({}, use) => {\n    seed();\n    await use(new Cart());\n  },",
        ),
        (
            "the fixture depends on another",
            "cart: async ({ page }, use) => {\n    await use(new Cart());\n  },",
        ),
        (
            "the instance comes from a factory",
            "cart: async ({}, use) => {\n    await use(makeCart());\n  },",
        ),
        (
            "the argument is not a literal",
            "cart: async ({}, use) => {\n    await use(new Cart(seed()));\n  },",
        ),
        (
            "the constructor is a namespace member",
            "cart: async ({}, use) => {\n    await use(new shop.Cart());\n  },",
        ),
        (
            "the use callback is not awaited",
            "cart: async ({}, use) => {\n    use(new Cart());\n  },",
        ),
        (
            "an option tuple",
            "cart: [async ({}, use) => {\n    await use(new Cart());\n  }, { auto: true }],",
        ),
        (
            "a method shorthand",
            "async cart({}, use) {\n    await use(new Cart());\n  },",
        ),
        (
            "a computed key",
            "[\"cart\"]: async ({}, use) => {\n    await use(new Cart());\n  },",
        ),
        ("a spread of other fixtures", "...others,"),
    ] {
        let fixtures = PLAIN.replace(
            "cart: async ({}, use) => {\n    await use(new Cart());\n  },",
            fixture,
        );
        assert_ne!(fixtures, PLAIN, "{label}: the case did not apply");
        assert_eq!(
            constructors(&spec(&fixtures, USES), "cart"),
            vec![None],
            "{label}"
        );
    }
}

#[test]
fn a_file_that_overrides_or_merges_fixtures_binds_nothing() {
    for (label, extra) in [
        ("a use override", "test.use(overrides);"),
        ("a spaced use override", "test.use (overrides);"),
        ("an optional-call use override", "test.use?.(overrides);"),
        ("a bracketed use override", "test[\"use\"](overrides);"),
        ("a second extend", "const other = test.extend({});"),
        ("a merge", "const merged = mergeTests(test, other);"),
        ("a template-literal key", "test[`use`](overrides);"),
        ("a spaced member", "test . use(overrides);"),
        ("an alias", "const alias = test;\nalias.use(overrides);"),
        ("a wrapper", "install(test);"),
        ("a re-export", "export { test };"),
    ] {
        let source = spec(PLAIN, &format!("{extra}\n{USES}"));
        assert_eq!(constructors(&source, "cart"), vec![None], "{label}");
    }
}

#[test]
fn prose_and_registration_members_do_not_withhold() {
    let tests = "test.describe(\"a test of carts\", () => {\n  // the test then\n  test(\"adds\", ({ cart }) => {\n    cart.add(250);\n  });\n});";
    assert_eq!(
        constructors(&spec(PLAIN, tests), "cart"),
        vec![Some("Cart".to_string())]
    );
}

#[test]
fn a_registration_that_is_not_the_extended_test_binds_nothing() {
    for (label, tests) in [
        (
            "test.only is another callee",
            "test.only(\"adds\", ({ cart }) => {\n  cart.add(250);\n});",
        ),
        (
            "a fixture key the file does not define",
            "test(\"adds\", ({ page: cart }) => {\n  cart.add(250);\n});",
        ),
        (
            "a default value on the parameter",
            "test(\"adds\", ({ cart = seeded }) => {\n  cart.add(250);\n});",
        ),
        (
            "a nested pattern",
            "test(\"adds\", ({ cart: { total } }) => {\n  total();\n});",
        ),
        (
            "a computed key",
            "test(\"adds\", ({ [key]: cart }) => {\n  cart.add(250);\n});",
        ),
        (
            "the parameter is reassigned in the body",
            "test(\"adds\", ({ cart }) => {\n  cart = seeded;\n  cart.add(250);\n});",
        ),
        (
            "a hook takes the same fixture and may seed it",
            "test.beforeEach(({ cart }) => {\n  cart.add(1);\n});\ntest(\"adds\", ({ cart }) => {\n  cart.add(250);\n});",
        ),
    ] {
        assert_eq!(
            constructors(&spec(PLAIN, tests), "cart"),
            vec![None],
            "{label}"
        );
    }
}

#[test]
fn a_test_not_imported_from_playwright_binds_nothing() {
    let source = format!(
        "import {{ expect, test as base }} from \"./harness\";\nimport {{ Cart }} from \"../src/cart\";\n{PLAIN}\n{USES}\n"
    );
    // The file may not even read as a Playwright file; either way no
    // binding names a constructor.
    assert!(constructors(&source, "cart").iter().all(Option::is_none));
}

#[test]
fn a_class_rebound_in_the_file_binds_nothing() {
    let source = spec(PLAIN, &format!("Cart.prototype.add = spy;\n{USES}"));
    assert_eq!(constructors(&source, "cart"), vec![None]);
}

mod relation {
    use super::super::super::owners::extract_owners;
    use super::super::super::related_tests::receiver_owner_call_relation;
    use super::{PLAIN, spec};
    use crate::analysis::language::typescript::tests_extract::extract_tests;
    use std::path::Path;

    const CART: &str = "export class Cart {\n  private cents: number[] = [];\n  add(amount: number): void {\n    this.cents.push(amount);\n  }\n}\n";

    /// Whether the registration at `index` of `tests` reaches `Cart.add`
    /// through the receiver relation.
    fn reaches_add(tests: &str, index: usize) -> bool {
        let owners = extract_owners(Path::new("src/cart.ts"), CART);
        let adds: Vec<_> = owners.iter().filter(|owner| owner.name == "add").collect();
        assert_eq!(adds.len(), 1, "the fixture class has one `add` owner");
        let extracted = extract_tests(Path::new("test/cart.spec.ts"), &spec(PLAIN, tests));
        adds.iter()
            .any(|owner| receiver_owner_call_relation(&extracted[index], owner, None, None))
    }

    const ADDS: &str = "test(\"adds\", ({ cart }) => {\n  cart.add(250);\n});";

    #[test]
    fn a_call_through_the_fixture_parameter_reaches_the_method() {
        assert!(reaches_add(ADDS, 0));
        assert!(reaches_add(
            "test(\"adds\", ({ cart: basket }) => {\n  basket.add(250);\n});",
            0
        ));
    }

    #[test]
    fn a_body_that_shadows_or_writes_the_parameter_does_not() {
        for (label, tests) in [
            (
                "a local declaration",
                "test(\"adds\", ({ cart }) => {\n  {\n    const cart = other();\n    cart.add(250);\n  }\n});",
            ),
            (
                "a hook that takes the fixture first",
                "test.beforeEach(({ cart }) => {\n  cart.add(1);\n});\ntest(\"adds\", ({ cart }) => {\n  cart.add(250);\n});",
            ),
            (
                "a later local class of the same name",
                "class Cart {}\ntest(\"adds\", ({ cart }) => {\n  cart.add(250);\n});",
            ),
        ] {
            assert!(!reaches_add(tests, 0), "{label}");
        }
    }
}

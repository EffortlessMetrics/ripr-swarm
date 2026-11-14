//! A class a CommonJS test imports with `require` (#14088).
//!
//! `const { Cart } = require('../src/cart')` is at once the import and a
//! file-level binding named `Cart`. That binding is the import itself, not a
//! rebind, so `new Cart()` constructs the owner's class and a method called
//! on the receiver relates, as it does for `import { Cart }`.
//!
//! * The named `require` forms relate a method and the constructor.
//! * A name the test file really rebinds stays unrelated: a re-declared
//!   `var`, a reassigned `let`, a file-level local class, a `describe`-level
//!   class, another module's class, a mutated `require` namespace.
//! * A `var` import that another file-level declaration re-declares is not
//!   recorded as an import at all, for a class or a function.
//!
//! Every case relates through `find_related_tests`, the production path, after
//! asserting the parsed subjects, and every negative has a credited twin that
//! differs only in the import line.

use super::*;
use crate::domain::RelationReason;

const CART: &str = "class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add(price) {\n    this.items.push(price);\n  }\n  total() {\n    let sum = 0;\n    for (const p of this.items) sum += p;\n    if (sum >= 100) {\n      return sum - 10;\n    }\n    return sum;\n  }\n}\nmodule.exports = { Cart };\n";
const OTHER_CART: &str = "class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add() {}\n  total() {\n    return 110;\n  }\n}\nmodule.exports = { Cart };\n";
const LOCAL_CART: &str = "class { add() {} total() { return 110; } }";

const BODY: &str = "    const cart = new Cart();\n    cart.add(60);\n    cart.add(60);\n    expect(cart.total()).toBe(110);";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Total,
    Constructor,
}

fn test_source(preamble: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('Cart', () => {{\n  test('total applies the bulk rebate', () => {{\n{BODY}\n  }});\n}});\n"
    )
}

fn relation(source: &str, subject: Subject) -> Result<Option<RelationReason>, String> {
    let files = [("src/cart.js", CART), ("src/other.js", OTHER_CART)];
    let test_file = Path::new("tests/cart.test.js");
    let mut workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    workspace_files.push(test_file.to_path_buf());
    let mut sources = std::collections::HashMap::new();
    let mut owners = Vec::new();
    for (path, text) in files {
        sources.insert(PathBuf::from(path), text.to_string());
        owners.extend(extract_owners(Path::new(path), text));
    }
    sources.insert(test_file.to_path_buf(), source.to_string());
    let index = ReExportIndex::build(
        &workspace_files,
        &sources,
        Path::new(""),
        None,
        |path: &Path| path.starts_with("tests"),
    );
    let wanted: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| {
            owner.file == Path::new("src/cart.js")
                && owner.class_name.as_deref() == Some("Cart")
                && match subject {
                    Subject::Total => owner.name == "total",
                    Subject::Constructor => owner.method_kind == TypeScriptMethodKind::Constructor,
                }
        })
        .collect();
    if wanted.len() != 1 {
        return Err(format!(
            "expected one {subject:?} owner of Cart in src/cart.js, got {}",
            wanted.len()
        ));
    }
    let tests = extract_tests(test_file, source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    if !tests[0].body_text.contains("cart.total()") {
        return Err(format!("test body lost the call: {:?}", tests[0].body_text));
    }
    let related = find_related_tests(wanted[0], &tests, None, &index, None);
    Ok(related.first().and_then(|test| test.relation_reason))
}

fn assert_credited(preamble: &str, subject: Subject) -> Result<(), String> {
    let related = relation(&test_source(preamble), subject)?;
    if related.is_none() {
        return Err(format!(
            "{preamble:?} / {subject:?}: expected a relation, got none"
        ));
    }
    Ok(())
}

fn assert_withheld(preamble: &str, subject: Subject) -> Result<(), String> {
    let related = relation(&test_source(preamble), subject)?;
    if related.is_some() {
        return Err(format!(
            "{preamble:?} / {subject:?}: expected no relation, got {related:?}"
        ));
    }
    Ok(())
}

const BOTH: [Subject; 2] = [Subject::Total, Subject::Constructor];

#[test]
fn a_named_require_of_the_owner_class_relates_like_an_esm_import() -> Result<(), String> {
    for preamble in [
        // The ESM twin, credited before #14088 as well.
        "import { Cart } from '../src/cart';",
        "const { Cart } = require('../src/cart');",
        "const Cart = require('../src/cart').Cart;",
        "let { Cart } = require('../src/cart');",
        "var { Cart } = require('../src/cart');",
    ] {
        for subject in BOTH {
            assert_credited(preamble, subject)?;
        }
    }
    Ok(())
}

#[test]
fn a_renamed_require_local_constructs_the_owner_class() -> Result<(), String> {
    let source = test_source("const { Cart: Basket } = require('../src/cart');")
        .replace("new Cart()", "new Basket()");
    for subject in BOTH {
        if relation(&source, subject)?.is_none() {
            return Err(format!("renamed local / {subject:?}: expected a relation"));
        }
    }
    Ok(())
}

#[test]
fn a_require_of_another_module_does_not_lend_its_class() -> Result<(), String> {
    for subject in BOTH {
        assert_withheld("const { Cart } = require('../src/other');", subject)?;
        assert_withheld("const Cart = require('../src/other').Cart;", subject)?;
    }
    Ok(())
}

#[test]
fn a_file_level_local_class_of_the_same_name_stays_unrelated() -> Result<(), String> {
    for subject in BOTH {
        assert_withheld(&format!("const Cart = {LOCAL_CART};"), subject)?;
    }
    Ok(())
}

#[test]
fn a_redeclared_var_import_stays_unrelated() -> Result<(), String> {
    for subject in BOTH {
        assert_withheld(
            &format!("var {{ Cart }} = require('../src/cart');\nvar Cart = {LOCAL_CART};"),
            subject,
        )?;
        assert_withheld(
            "var { Cart } = require('../src/cart');\nfunction Cart() {\n  this.add = () => {};\n  this.total = () => 110;\n}",
            subject,
        )?;
        // A sloppy block-level function hoists over the import too.
        assert_withheld(
            "var { Cart } = require('../src/cart');\nif (process.env.FAKE) {\n  function Cart() {\n    this.add = () => {};\n    this.total = () => 110;\n  }\n}",
            subject,
        )?;
    }
    Ok(())
}

#[test]
fn two_var_requires_of_one_local_stay_unrelated() -> Result<(), String> {
    for subject in BOTH {
        assert_withheld(
            "var { Cart } = require('../src/cart');\nvar { Cart } = require('../src/other');",
            subject,
        )?;
        assert_withheld(
            "var { Cart } = require('../src/other');\nvar { Cart } = require('../src/cart');",
            subject,
        )?;
    }
    Ok(())
}

#[test]
fn two_var_requires_of_the_owner_module_still_relate() -> Result<(), String> {
    // Two spellings of one module bind the same export twice.
    for subject in BOTH {
        assert_credited(
            "var { Cart } = require('../src/cart');\nvar { Cart } = require('../src/cart.js');",
            subject,
        )?;
    }
    Ok(())
}

#[test]
fn a_var_require_redeclared_by_an_unread_require_pattern_stays_unrelated() -> Result<(), String> {
    // The extractor records no import for a nested, rest or array pattern,
    // so each of these is another declaration of `Cart`, not a second import.
    for redeclaration in [
        "var { wrapper: { Cart } } = require('../src/other');",
        "var { ...Cart } = require('../src/other');",
        "var [Cart] = require('../src/other');",
        "var { wrapper: { Cart } } = require('../src/cart');",
    ] {
        for subject in BOTH {
            assert_withheld(
                &format!("var {{ Cart }} = require('../src/cart');\n{redeclaration}"),
                subject,
            )?;
            assert_withheld(
                &format!("{redeclaration}\nvar {{ Cart }} = require('../src/cart');"),
                subject,
            )?;
        }
    }
    Ok(())
}

#[test]
fn a_reassigned_let_import_stays_unrelated() -> Result<(), String> {
    for subject in BOTH {
        assert_withheld(
            &format!("let {{ Cart }} = require('../src/cart');\nCart = {LOCAL_CART};"),
            subject,
        )?;
    }
    Ok(())
}

#[test]
fn a_describe_level_class_shadows_the_require() -> Result<(), String> {
    let shadowed = format!(
        "const {{ Cart }} = require('../src/cart');\n\ndescribe('Cart', () => {{\n  const Cart = {LOCAL_CART};\n  test('total applies the bulk rebate', () => {{\n{BODY}\n  }});\n}});\n"
    );
    for subject in BOTH {
        let related = relation(&shadowed, subject)?;
        if related.is_some() {
            return Err(format!(
                "describe-level shadow / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_test_body_class_shadows_the_require() -> Result<(), String> {
    let shadowed = test_source("const { Cart } = require('../src/cart');").replace(
        "    const cart = new Cart();",
        &format!("    const Cart = {LOCAL_CART};\n    const cart = new Cart();"),
    );
    for subject in BOTH {
        let related = relation(&shadowed, subject)?;
        if related.is_some() {
            return Err(format!(
                "body shadow / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_require_namespace_keeps_the_strict_rule() -> Result<(), String> {
    // `const shop = require(...)` is a mutable object, so `new shop.Cart()`
    // is not credited from the file-level `shop` binding, mutated or not.
    for preamble in [
        "const shop = require('../src/cart');",
        &format!("const shop = require('../src/cart');\nshop.Cart = {LOCAL_CART};"),
    ] {
        let source = test_source(preamble).replace("new Cart()", "new shop.Cart()");
        for subject in BOTH {
            let related = relation(&source, subject)?;
            if related.is_some() {
                return Err(format!(
                    "{preamble:?} / {subject:?}: expected no relation, got {related:?}"
                ));
            }
        }
    }
    Ok(())
}

fn import_locals(source: &str) -> Result<Vec<String>, String> {
    let tests = extract_tests(Path::new("tests/price.test.js"), source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    Ok(tests[0]
        .imports_in_file
        .iter()
        .map(|import| import.local.clone())
        .collect())
}

#[test]
fn a_redeclared_var_require_is_not_an_import() -> Result<(), String> {
    const TEST: &str = "test('edge', () => {\n  expect(discount(100)).toBe(90);\n});\n";
    let plain = format!("var {{ discount }} = require('../src/price');\n{TEST}");
    assert_eq!(import_locals(&plain)?, vec!["discount".to_string()]);
    for redeclared in [
        "function discount() {\n  return 90;\n}\n",
        "var discount = () => 90;\n",
        "if (process.env.FAKE) {\n  var discount = () => 90;\n}\n",
    ] {
        let source = format!("var {{ discount }} = require('../src/price');\n{redeclared}{TEST}");
        assert_eq!(
            import_locals(&source)?,
            Vec::<String>::new(),
            "re-declared by {redeclared:?}"
        );
    }
    // Two requires of one name are both recorded: the admission scan reads
    // the package one, and relations compare the records.
    assert_eq!(
        import_locals(&format!(
            "var {{ discount }} = require('../src/price');\nvar {{ discount }} = require('tap');\n{TEST}"
        ))?,
        vec!["discount".to_string(), "discount".to_string()]
    );
    // Whole-module and member forms of the same `var` binding.
    for form in [
        "var discount = require('../src/price');\n",
        "var discount = require('../src/price').discount;\n",
    ] {
        assert_eq!(
            import_locals(&format!("{form}{TEST}"))?,
            vec!["discount".to_string()]
        );
        assert_eq!(
            import_locals(&format!(
                "{form}function discount() {{\n  return 90;\n}}\n{TEST}"
            ))?,
            Vec::<String>::new(),
            "re-declared {form:?}"
        );
    }
    Ok(())
}

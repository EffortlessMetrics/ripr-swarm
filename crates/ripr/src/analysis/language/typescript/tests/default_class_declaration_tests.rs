//! A class declared first and exported as the module default afterwards.
//!
//! `class Cart {}` followed by `export default Cart;` or
//! `export { Cart as default };` is the module's default export exactly like
//! `export default class Cart {}`, so `import Cart from '../src/cart'` binds
//! the owner's class and a method called on `new Cart()` relates.
//!
//! * Both separate-statement spellings relate a method and the constructor,
//!   under any local name the default import chooses.
//! * The class is only marked when the name provably still holds it: one
//!   file-level declaration, never written, and no named export of it.
//! * A class with a named export as well keeps its named-import reading and
//!   is not marked, so the default import of it stays a known limit.
//!
//! Every case relates through `find_related_tests`, the production path, after
//! asserting the parsed subjects, and every negative has a credited twin that
//! differs only in the export or import line.

use super::*;
use crate::domain::RelationReason;

const CLASS: &str = "class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add(price) {\n    this.items.push(price);\n  }\n  total() {\n    let sum = 0;\n    for (const p of this.items) sum += p;\n    if (sum >= 100) {\n      return sum - 10;\n    }\n    return sum;\n  }\n}\n";
const OTHER: &str = "export default class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add() {}\n  total() {\n    return 110;\n  }\n}\n";

const DEFAULT_IMPORT: &str = "import Cart from '../src/cart';";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Total,
    Constructor,
}

const BOTH: [Subject; 2] = [Subject::Total, Subject::Constructor];

fn owner_source(export_lines: &str) -> String {
    format!("{CLASS}{export_lines}\n")
}

fn test_source(preamble: &str, class_local: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('Cart', () => {{\n  test('total applies the bulk rebate', () => {{\n    const cart = new {class_local}();\n    cart.add(60);\n    cart.add(60);\n    expect(cart.total()).toBe(110);\n  }});\n}});\n"
    )
}

fn relation(
    owner_text: &str,
    test_text: &str,
    subject: Subject,
) -> Result<Option<RelationReason>, String> {
    let files = [("src/cart.js", owner_text), ("src/other.js", OTHER)];
    let test_file = Path::new("tests/cart.test.js");
    let mut workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    workspace_files.push(test_file.to_path_buf());
    let mut sources = std::collections::HashMap::new();
    let mut owners = Vec::new();
    for (path, text) in files {
        sources.insert(PathBuf::from(path), text.to_string());
        owners.extend(extract_owners(Path::new(path), text));
    }
    sources.insert(test_file.to_path_buf(), test_text.to_string());
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
    let tests = extract_tests(test_file, test_text);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    if !tests[0].body_text.contains("cart.total()") {
        return Err(format!("test body lost the call: {:?}", tests[0].body_text));
    }
    let related = find_related_tests(wanted[0], &tests, None, &index, None);
    Ok(related.first().and_then(|test| test.relation_reason))
}

fn assert_credited(export_lines: &str, preamble: &str, local: &str) -> Result<(), String> {
    for subject in BOTH {
        let related = relation(
            &owner_source(export_lines),
            &test_source(preamble, local),
            subject,
        )?;
        if related.is_none() {
            return Err(format!(
                "{export_lines:?} / {preamble:?} / {subject:?}: expected a relation, got none"
            ));
        }
    }
    Ok(())
}

fn assert_withheld(export_lines: &str, preamble: &str, local: &str) -> Result<(), String> {
    for subject in BOTH {
        let related = relation(
            &owner_source(export_lines),
            &test_source(preamble, local),
            subject,
        )?;
        if related.is_some() {
            return Err(format!(
                "{export_lines:?} / {preamble:?} / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

fn marked_default(export_lines: &str) -> Result<bool, String> {
    let text = owner_source(export_lines);
    let owners = extract_owners(Path::new("src/cart.js"), &text);
    let methods: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| owner.class_name.as_deref() == Some("Cart"))
        .collect();
    if methods.len() < 3 {
        return Err(format!(
            "expected the constructor, add and total of Cart, got {}",
            methods.len()
        ));
    }
    let marked = methods
        .iter()
        .filter(|owner| owner.class_default_export)
        .count();
    if marked != 0 && marked != methods.len() {
        return Err(format!(
            "{export_lines:?}: {marked} of {} Cart members marked",
            methods.len()
        ));
    }
    Ok(marked != 0)
}

#[test]
fn a_class_exported_as_default_by_a_later_statement_relates_a_default_import() -> Result<(), String>
{
    for export_lines in [
        "export default Cart;",
        "export { Cart as default };",
        "export { Cart as \"default\" };",
    ] {
        if !marked_default(export_lines)? {
            return Err(format!("{export_lines:?}: class not marked as default"));
        }
        assert_credited(export_lines, DEFAULT_IMPORT, "Cart")?;
    }
    Ok(())
}

#[test]
fn the_default_import_may_choose_any_local_name() -> Result<(), String> {
    assert_credited(
        "export default Cart;",
        "import Basket from '../src/cart';",
        "Basket",
    )?;
    assert_credited(
        "export { Cart as default };",
        "import { default as Basket } from '../src/cart';",
        "Basket",
    )
}

#[test]
fn the_inline_default_class_twin_still_relates() -> Result<(), String> {
    let inline = CLASS.replacen("class Cart", "export default class Cart", 1);
    for subject in BOTH {
        if relation(&inline, &test_source(DEFAULT_IMPORT, "Cart"), subject)?.is_none() {
            return Err(format!(
                "inline default class / {subject:?}: expected a relation"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_default_import_of_another_module_does_not_lend_its_class() -> Result<(), String> {
    assert_withheld(
        "export default Cart;",
        "import Cart from '../src/other';",
        "Cart",
    )
}

#[test]
fn a_named_import_of_a_default_only_class_stays_unrelated() -> Result<(), String> {
    // The module has no export named `Cart`; the import binds nothing.
    assert_withheld(
        "export default Cart;",
        "import { Cart } from '../src/cart';",
        "Cart",
    )
}

#[test]
fn a_class_that_is_not_the_default_export_stays_unrelated_to_a_default_import() -> Result<(), String>
{
    for export_lines in [
        // Another value is the default export.
        "const fallback = { total() { return 110; } };\nexport default fallback;\nexport { Cart as Basket };",
        // No default export at all.
        "export const version = 1;",
    ] {
        if marked_default(export_lines)? {
            return Err(format!("{export_lines:?}: class marked as default"));
        }
        assert_withheld(export_lines, DEFAULT_IMPORT, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_rewritten_class_binding_is_not_marked() -> Result<(), String> {
    for export_lines in [
        // `export { Cart as default }` is a live binding and a class
        // declaration is assignable, so the default may be another class.
        "Cart = class { add() {} total() { return 110; } };\nexport { Cart as default };",
        "Cart = class { add() {} total() { return 110; } };\nexport default Cart;",
        "function swap() {\n  Cart = null;\n}\nexport default Cart;",
    ] {
        if marked_default(export_lines)? {
            return Err(format!("{export_lines:?}: rewritten class marked"));
        }
        assert_withheld(export_lines, DEFAULT_IMPORT, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_class_with_a_named_export_too_keeps_its_named_import_reading() -> Result<(), String> {
    for export_lines in [
        "export { Cart };\nexport default Cart;",
        "export { Cart, Cart as default };",
    ] {
        if marked_default(export_lines)? {
            return Err(format!(
                "{export_lines:?}: marker would withdraw the named import"
            ));
        }
        assert_credited(export_lines, "import { Cart } from '../src/cart';", "Cart")?;
    }
    Ok(())
}

#[test]
fn a_type_only_default_export_does_not_mark_the_class() -> Result<(), String> {
    let text = format!("{CLASS}export type {{ Cart as default }};\n");
    let owners = extract_owners(Path::new("src/cart.ts"), &text);
    let methods: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| owner.class_name.as_deref() == Some("Cart"))
        .collect();
    if methods.is_empty() {
        return Err("no Cart members parsed".to_string());
    }
    if methods.iter().any(|owner| owner.class_default_export) {
        return Err("a type-only default export marked the class".to_string());
    }
    Ok(())
}

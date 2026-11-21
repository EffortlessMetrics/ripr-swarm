//! Barrels that import a binding and export it in a separate statement
//! (RIPR-SPEC-0095). `import { Help } from "./help"; export { Help };` exports
//! the binding of `./help` exactly as `export { Help } from "./help"` does (an
//! indirect binding in ECMAScript), so a test that imports the name from the
//! barrel reaches the changed owner. The shape is commander.js's `index.js`,
//! where every tested class read `no_static_path`.
//!
//! Every case builds the index through `ReExportIndex::build` over real
//! sources, applies the import rewrites the analysis does, and relates through
//! `find_related_tests`, the production path. Each case first asserts the
//! parsed subjects (one test, one owner), and every negative has a credited
//! twin differing only in the property under test.

use super::*;
use crate::domain::RelationReason;

const UTILS: &str = "export function withoutBase(input: string, base: string): string {\n  return input.slice(base.length);\n}\n";
const DEFAULT_UTILS: &str = "export default function withoutBase(input: string, base: string): string {\n  return input.slice(base.length);\n}\n";
const HELP: &str = "export class Help {\n  visibleOptions(cmd: { options: string[] }): string[] {\n    return cmd.options.filter((option) => option !== \"\");\n  }\n}\n";

const CALL_TEST: &str = "import { expect, test } from \"vitest\";\nimport { withoutBase } from \"../src\";\n\ntest(\"strips the base\", () => {\n  expect(withoutBase(\"/foo/bar\", \"/foo\")).toBe(\"/bar\");\n});\n";
const HELP_TEST: &str = "import { expect, test } from \"vitest\";\nimport { Help } from \"../src\";\n\ntest(\"lists options\", () => {\n  const helper = new Help();\n  expect(helper.visibleOptions({ options: [\"a\"] })).toEqual([\"a\"]);\n});\n";

/// The relation the single test parsed from `test_source` has to the owner
/// `owner_name` declared in `owner_file`, or `None` when unrelated.
fn relation(
    files: &[(&str, &str)],
    owner_file: &str,
    owner_name: &str,
    test_source: &str,
) -> Result<Option<RelationReason>, String> {
    let test_file = Path::new("tests/barrel.test.ts");
    let mut workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    workspace_files.push(test_file.to_path_buf());
    let mut sources = std::collections::HashMap::new();
    let mut owners = Vec::new();
    for (path, text) in files {
        sources.insert(PathBuf::from(path), (*text).to_string());
        owners.extend(extract_owners(Path::new(path), text));
    }
    sources.insert(test_file.to_path_buf(), test_source.to_string());
    let index = ReExportIndex::build(
        &workspace_files,
        &sources,
        Path::new(""),
        None,
        |path: &Path| path.starts_with("tests"),
    );
    let wanted: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| owner.file == Path::new(owner_file) && owner.name == owner_name)
        .collect();
    if wanted.len() != 1 {
        return Err(format!(
            "expected one owner `{owner_name}` in {owner_file}, got {}",
            wanted.len()
        ));
    }
    let mut tests = extract_tests(test_file, test_source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    index.rewrite_namespace_reexport_imports(&mut tests, None, None);
    index.rewrite_class_barrel_imports(
        &mut tests,
        &related_tests::ExportedClasses::from_owners(&owners),
        None,
        None,
    );
    let related = find_related_tests(wanted[0], &tests, None, &index, None);
    Ok(related.first().and_then(|test| test.relation_reason))
}

const CHAIN: Option<RelationReason> = Some(RelationReason::ReExportChainFollowed);

#[test]
fn an_import_then_export_barrel_credits_the_changed_function() -> Result<(), String> {
    let split = [
        ("src/utils.ts", UTILS),
        (
            "src/index.ts",
            "import { withoutBase } from \"./utils\";\nexport { withoutBase };\n",
        ),
    ];
    assert_eq!(
        relation(&split, "src/utils.ts", "withoutBase", CALL_TEST)?,
        CHAIN,
        "import then export must read as the re-export it is"
    );
    // Control: the one-statement spelling of the same barrel.
    let joined = [
        ("src/utils.ts", UTILS),
        ("src/index.ts", "export { withoutBase } from \"./utils\";\n"),
    ];
    assert_eq!(
        relation(&joined, "src/utils.ts", "withoutBase", CALL_TEST)?,
        CHAIN,
        "control: export-from barrel"
    );
    Ok(())
}

#[test]
fn renames_on_the_import_and_on_the_export_resolve_to_the_owner() -> Result<(), String> {
    let files = [
        ("src/utils.ts", UTILS),
        (
            "src/index.ts",
            "import { withoutBase as strip } from \"./utils\";\nexport { strip as stripBase };\n",
        ),
    ];
    let test = CALL_TEST.replace("withoutBase", "stripBase");
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", &test)?,
        CHAIN,
        "the exported alias names the owner's binding"
    );
    // The barrel does not export the import's local alias.
    let wrong = CALL_TEST.replace("withoutBase", "strip");
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", &wrong)?,
        None,
        "a name the barrel never exports must not relate"
    );
    Ok(())
}

#[test]
fn a_default_import_then_export_credits_the_default_exported_owner() -> Result<(), String> {
    let files = [
        ("src/utils.ts", DEFAULT_UTILS),
        (
            "src/index.ts",
            "import withoutBase from \"./utils\";\nexport { withoutBase };\n",
        ),
    ];
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", CALL_TEST)?,
        CHAIN,
        "a default import exported by name lands on the owner's default export"
    );
    // The same barrel over a module with no default export binds nothing.
    let named = [("src/utils.ts", UTILS), files[1]];
    assert_eq!(
        relation(&named, "src/utils.ts", "withoutBase", CALL_TEST)?,
        None,
        "a default import of a module without a default export must not relate"
    );
    Ok(())
}

#[test]
fn an_import_then_export_barrel_credits_a_class_receiver() -> Result<(), String> {
    let files = [
        ("src/help.ts", HELP),
        (
            "src/index.ts",
            "import { Help } from \"./help\";\nexport { Help };\n",
        ),
    ];
    assert_eq!(
        relation(&files, "src/help.ts", "visibleOptions", HELP_TEST)?,
        CHAIN,
        "the commander.js shape: a class exported after its import"
    );
    Ok(())
}

#[test]
fn a_namespace_import_then_export_credits_a_member_call() -> Result<(), String> {
    let files = [
        ("src/utils.ts", UTILS),
        (
            "src/index.ts",
            "import * as utils from \"./utils\";\nexport { utils };\n",
        ),
    ];
    let test = "import { expect, test } from \"vitest\";\nimport { utils } from \"../src\";\n\ntest(\"strips the base\", () => {\n  expect(utils.withoutBase(\"/foo/bar\", \"/foo\")).toBe(\"/bar\");\n});\n";
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", test)?,
        CHAIN,
        "an exported namespace import reads as `export * as utils from`"
    );
    Ok(())
}

#[test]
fn an_import_from_another_module_does_not_credit_the_same_named_owner() -> Result<(), String> {
    // Twin of the credited case: only the import's source differs.
    let files = [
        ("src/utils.ts", UTILS),
        ("src/other.ts", UTILS),
        (
            "src/index.ts",
            "import { withoutBase } from \"./other\";\nexport { withoutBase };\n",
        ),
    ];
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", CALL_TEST)?,
        None,
        "the barrel exports ./other's binding, not the changed owner's"
    );
    assert_eq!(
        relation(&files, "src/other.ts", "withoutBase", CALL_TEST)?,
        CHAIN,
        "the module the barrel imports from is the one credited"
    );
    Ok(())
}

#[test]
fn a_package_import_then_export_does_not_credit_a_workspace_owner() -> Result<(), String> {
    let files = [
        ("src/utils.ts", UTILS),
        (
            "src/index.ts",
            "import { withoutBase } from \"ufo\";\nexport { withoutBase };\n",
        ),
    ];
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", CALL_TEST)?,
        None,
        "a package binding exported by the barrel is not the workspace owner"
    );
    Ok(())
}

#[test]
fn a_type_only_import_is_not_a_value_hop() -> Result<(), String> {
    let files = [
        ("src/utils.ts", UTILS),
        (
            "src/index.ts",
            "import type { withoutBase } from \"./utils\";\nexport { withoutBase };\n",
        ),
    ];
    assert_eq!(
        relation(&files, "src/utils.ts", "withoutBase", CALL_TEST)?,
        None,
        "a type-only import binds no value to export"
    );
    Ok(())
}

#[test]
fn an_exported_import_resolves_on_disk_so_its_facts_are_not_persisted() -> Result<(), String> {
    let facts = |source: &str| {
        super::super::extraction_cache::file_facts_cached(
            Path::new("src/index.ts"),
            source,
            Path::new("."),
            None,
        )
        .map(|facts| facts.export_facts.clone())
    };
    let hop = facts("import { withoutBase } from \"./utils\";\nexport { withoutBase };\n")?;
    assert!(
        hop.re_exports_from_source,
        "the import target resolves on disk, like `export ... from`"
    );
    assert_eq!(
        hop.entries,
        vec![(
            ("src/index".to_string(), "withoutBase".to_string()),
            ("withoutBase".to_string(), "src/utils".to_string())
        )]
    );
    assert!(hop.local_exports.is_empty(), "not an own declaration");
    // Control: an own binding exported by list stays a disk-independent own export.
    let own = facts("const withoutBase = 1;\nexport { withoutBase };\n")?;
    assert!(!own.re_exports_from_source);
    assert_eq!(own.local_exports, vec!["withoutBase".to_string()]);
    Ok(())
}

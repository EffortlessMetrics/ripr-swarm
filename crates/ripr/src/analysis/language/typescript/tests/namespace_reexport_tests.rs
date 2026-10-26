//! Namespace re-exports (RIPR-SPEC-0095). `export * as units from "./units"`
//! binds the namespace object of `./units`, so a test that imports `units`
//! by name from the barrel and calls `units.cmToIn(...)` reaches `cmToIn`
//! exactly as `import * as units from "./units"` would, and discloses the
//! barrel hop as `re_export_chain_followed`. Anything that does not land on
//! the owner's module (another namespace, another member, a mocked barrel, a
//! shadowing local, an ambiguous or cyclic chain) stays uncredited.
//!
//! Every case builds the index through `ReExportIndex::build` over real
//! sources, applies the same import rewrite the analysis does, and relates
//! through `find_related_tests`, the production path.

use super::*;
use crate::domain::RelationReason;

const UNITS: &str = "export function cmToIn(cm: number): number {\n  return cm / 2.5;\n}\n\nexport function inToCm(inches: number): number {\n  return inches * 2.54;\n}\n";
const OTHER: &str = "export function cmToIn(cm: number): number {\n  return cm * 0.4;\n}\n";

fn test_source(body: &str) -> String {
    format!(
        "import {{ expect, it }} from \"vitest\";\nimport {{ units }} from \"../src/barrel\";\n\nit(\"converts\", () => {{\n{body}\n}});\n"
    )
}

/// Related tests of `cmToIn` in `src/units.ts` for the single test parsed
/// from `source` in `tests/barrel.test.ts`.
fn related_through_namespace(
    files: &[(&str, &str)],
    source: &str,
) -> Result<Vec<RelatedTest>, String> {
    let test_file = Path::new("tests/barrel.test.ts");
    let mut workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    workspace_files.push(test_file.to_path_buf());
    let mut sources = std::collections::HashMap::new();
    for (path, text) in files {
        sources.insert(PathBuf::from(path), (*text).to_string());
    }
    sources.insert(test_file.to_path_buf(), source.to_string());
    let index = ReExportIndex::build(
        &workspace_files,
        &sources,
        Path::new(""),
        None,
        |path: &Path| path.starts_with("tests"),
    );
    let owner = extract_owners(Path::new("src/units.ts"), UNITS)
        .into_iter()
        .find(|owner| owner.name == "cmToIn")
        .ok_or("owner cmToIn was not extracted")?;
    let mut tests = extract_tests(test_file, source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    index.rewrite_namespace_reexport_imports(&mut tests, None, None);
    Ok(find_related_tests(&owner, &tests, None, &index, None))
}

fn assert_credited(related: &[RelatedTest], label: &str) {
    assert_eq!(related.len(), 1, "{label}: the namespace call must relate");
    assert_eq!(
        related.first().and_then(|test| test.relation_reason),
        Some(RelationReason::ReExportChainFollowed),
        "{label}: disclosed as a re-export chain"
    );
}

fn assert_not_credited(related: &[RelatedTest], label: &str) {
    assert!(
        related
            .iter()
            .all(|test| test.relation_reason != Some(RelationReason::ReExportChainFollowed)),
        "{label}: must not be credited through the namespace re-export"
    );
}

const CALL: &str = "  expect(units.cmToIn(2.54)).toBe(1);";

#[test]
fn a_namespace_re_export_credits_a_member_call() -> Result<(), String> {
    let files = [
        ("src/units.ts", UNITS),
        ("src/barrel.ts", "export * as units from \"./units\";\n"),
    ];
    assert_credited(
        &related_through_namespace(&files, &test_source(CALL))?,
        "file barrel",
    );
    Ok(())
}

#[test]
fn an_index_barrel_and_a_re_export_hop_reach_the_namespace() -> Result<(), String> {
    let directory = [
        ("src/units.ts", UNITS),
        (
            "src/barrel/index.ts",
            "export * as units from \"../units\";\n",
        ),
    ];
    let source = test_source(CALL).replace("../src/barrel", "../src/barrel/index");
    assert_credited(
        &related_through_namespace(&directory, &source)?,
        "index barrel",
    );
    let star = [
        ("src/units.ts", UNITS),
        ("src/inner.ts", "export * as units from \"./units\";\n"),
        ("src/barrel.ts", "export * from \"./inner\";\n"),
    ];
    assert_credited(
        &related_through_namespace(&star, &test_source(CALL))?,
        "star hop",
    );
    let named = [
        ("src/units.ts", UNITS),
        ("src/inner.ts", "export * as units from \"./units\";\n"),
        ("src/barrel.ts", "export { units } from \"./inner\";\n"),
    ];
    assert_credited(
        &related_through_namespace(&named, &test_source(CALL))?,
        "named hop",
    );
    Ok(())
}

#[test]
fn a_namespace_of_another_module_is_not_the_owner() -> Result<(), String> {
    let files = [
        ("src/units.ts", UNITS),
        ("src/other.ts", OTHER),
        ("src/barrel.ts", "export * as units from \"./other\";\n"),
    ];
    assert_not_credited(
        &related_through_namespace(&files, &test_source(CALL))?,
        "same-named function in another module",
    );
    Ok(())
}

#[test]
fn another_member_or_a_bare_reference_is_not_a_call_of_the_owner() -> Result<(), String> {
    let files = [
        ("src/units.ts", UNITS),
        ("src/barrel.ts", "export * as units from \"./units\";\n"),
    ];
    for (label, body) in [
        ("other member", "  expect(units.inToCm(1)).toBe(2.54);"),
        ("no call", "  expect(typeof units).toBe(\"object\");"),
    ] {
        assert_not_credited(
            &related_through_namespace(&files, &test_source(body))?,
            label,
        );
    }
    Ok(())
}

#[test]
fn a_mocked_barrel_or_target_replaces_the_namespace() -> Result<(), String> {
    let files = [
        ("src/units.ts", UNITS),
        ("src/barrel.ts", "export * as units from \"./units\";\n"),
    ];
    for mocked in ["../src/barrel", "../src/units"] {
        let source = format!(
            "import {{ expect, it, vi }} from \"vitest\";\nimport {{ units }} from \"../src/barrel\";\n\nvi.mock(\"{mocked}\");\n\nit(\"converts\", () => {{\n{CALL}\n}});\n"
        );
        assert_not_credited(
            &related_through_namespace(&files, &source)?,
            &format!("mock of {mocked}"),
        );
    }
    Ok(())
}

#[test]
fn a_shadowing_local_is_not_the_namespace() -> Result<(), String> {
    let files = [
        ("src/units.ts", UNITS),
        ("src/barrel.ts", "export * as units from \"./units\";\n"),
    ];
    let body = format!("  const units = {{ cmToIn: (cm: number) => cm }};\n{CALL}");
    assert_not_credited(
        &related_through_namespace(&files, &test_source(&body))?,
        "body-local units",
    );
    Ok(())
}

#[test]
fn an_ambiguous_or_cyclic_chain_is_not_followed() -> Result<(), String> {
    let ambiguous = [
        ("src/units.ts", UNITS),
        ("src/other.ts", OTHER),
        ("src/a.ts", "export * as units from \"./units\";\n"),
        ("src/b.ts", "export * as units from \"./other\";\n"),
        (
            "src/barrel.ts",
            "export * from \"./a\";\nexport * from \"./b\";\n",
        ),
    ];
    assert_not_credited(
        &related_through_namespace(&ambiguous, &test_source(CALL))?,
        "two star sources with different namespaces",
    );
    let competing = [
        ("src/units.ts", UNITS),
        ("src/a.ts", "export * as units from \"./units\";\n"),
        (
            "src/b.ts",
            "export const units = { cmToIn: (cm: number) => cm };\n",
        ),
        (
            "src/barrel.ts",
            "export * from \"./a\";\nexport * from \"./b\";\n",
        ),
    ];
    assert_not_credited(
        &related_through_namespace(&competing, &test_source(CALL))?,
        "a namespace star and an ordinary star export of the same name",
    );
    let cycle = [
        ("src/units.ts", UNITS),
        ("src/a.ts", "export { units } from \"./barrel\";\n"),
        ("src/barrel.ts", "export { units } from \"./a\";\n"),
    ];
    assert_not_credited(
        &related_through_namespace(&cycle, &test_source(CALL))?,
        "cycle",
    );
    Ok(())
}

#[test]
fn a_type_only_namespace_export_binds_no_value() -> Result<(), String> {
    let files = [
        ("src/units.ts", UNITS),
        (
            "src/barrel.ts",
            "export type * as units from \"./units\";\n",
        ),
    ];
    assert_not_credited(
        &related_through_namespace(&files, &test_source(CALL))?,
        "type-only namespace export",
    );
    Ok(())
}

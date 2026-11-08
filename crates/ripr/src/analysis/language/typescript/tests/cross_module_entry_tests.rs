//! Owners reached through an export of ANOTHER module (#13592, unjs/ufo): a
//! test calls `normalize` (module `b`), which calls `decodeThing` imported from
//! module `a`. The same-module caller chain never crossed an import, so the
//! owner read `no_static_path` though the test runs it. The chain now follows
//! the importer's binding through the existing import / re-export identity,
//! inside the same three-hop bound, and reads exactly like a same-module
//! entry: indirect reach, `static_unknown`, never `exposed`, no repair start.
//!
//! Every negative asserts its positive twin first, so the refusal is the
//! single changed element and not a parse or resolution accident.

use super::*;
use crate::analysis::language::typescript::related_tests::ReExportIndex;
use std::collections::HashMap;

const A: &str = "export function decodeThing(text: string): string {\n  return decodeURIComponent(text.replace(/%252F/g, \"%2F\"));\n}\n";
const A_LINE: &str = "  return decodeURIComponent(text.replace(/%252F/g, \"%2F\"));";
const B: &str = "import { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  return \"/\" + decodeThing(p);\n}\n";
const B_TEST: &str = "import { describe, expect, test } from \"vitest\";\nimport { normalize } from \"../src/b\";\ndescribe(\"normalize\", () => {\n  test(\"decodes\", () => {\n    expect(normalize(\"a%252Fb\")).toBe(\"/a%2Fb\");\n  });\n});\n";

type Files<'a> = Vec<(&'a str, String)>;

fn base() -> Files<'static> {
    vec![
        ("src/a.ts", A.to_string()),
        ("src/b.ts", B.to_string()),
        ("tests/b.test.ts", B_TEST.to_string()),
    ]
}

fn with(mut files: Files<'static>, path: &'static str, source: String) -> Files<'static> {
    match files.iter_mut().find(|(name, _)| *name == path) {
        Some(file) => file.1 = source,
        None => files.push((path, source)),
    }
    files
}

fn finding_at(
    label: &str,
    files: &Files<'_>,
    changed_path: &str,
    changed_line: &str,
    owner: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let mut changed_source = None;
    for (path, source) in files {
        ts_write_file(&root.join(path), source)?;
        if *path == changed_path {
            changed_source = Some(source.as_str());
        }
    }
    let line = changed_source
        .and_then(|source| source.lines().position(|line| line == changed_line))
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from {changed_path}"))?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(changed_path, &[(line, changed_line)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let suffix = format!("::{owner}");
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|candidate| candidate.0.ends_with(&suffix))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `{owner}`"))
}

fn finding(label: &str, files: &Files<'_>) -> Result<Finding, String> {
    finding_at(label, files, "src/a.ts", A_LINE, "decodeThing")
}

fn relates_through_entry(finding: &Finding) -> bool {
    finding
        .related_tests
        .iter()
        .any(|test| test.relation_reason == Some(crate::domain::RelationReason::HelperOwnerCall))
}

/// The owners each non-test file parses to: fixture construction, asserted
/// before any downstream claim.
fn assert_parsed(label: &str, files: &Files<'_>, expected: &[(&str, &str)]) {
    for (path, name) in expected {
        let source = files
            .iter()
            .find(|(file, _)| file == path)
            .map(|(_, source)| source.as_str())
            .unwrap_or_default();
        assert!(
            extract_owners(Path::new(path), source)
                .iter()
                .any(|owner| owner.name == *name),
            "{label}: `{name}` must parse as an owner of {path}"
        );
    }
}

/// The cross-module entries `owner` of `owner_file` resolves to over `files`.
fn cross_entries(
    files: &Files<'_>,
    owner_file: &str,
    owner_name: &str,
) -> Vec<(String, u8, PathBuf)> {
    let workspace_files: Vec<PathBuf> = files.iter().map(|(path, _)| PathBuf::from(path)).collect();
    let sources: HashMap<PathBuf, String> = files
        .iter()
        .map(|(path, source)| (PathBuf::from(path), source.clone()))
        .collect();
    let index = ReExportIndex::build(&workspace_files, &sources, Path::new("."), None, |path| {
        path.starts_with("tests")
    });
    let source = files
        .iter()
        .find(|(path, _)| *path == owner_file)
        .map(|(_, source)| source.as_str())
        .unwrap_or_default();
    let Some(owner) = extract_owners(Path::new(owner_file), source)
        .into_iter()
        .find(|owner| owner.name == owner_name)
    else {
        return Vec::new();
    };
    let mut entries: Vec<(String, u8, PathBuf)> = index
        .cross_module_entries(&owner, None, None)
        .entries
        .into_iter()
        .map(|entry| {
            let file = entry.cross_module.map(|link| link.file).unwrap_or_default();
            (entry.name, entry.hops, file)
        })
        .collect();
    entries.sort();
    entries
}

fn assert_related(label: &str, files: &Files<'_>) -> Result<(), String> {
    let finding = finding(label, files)?;
    assert!(
        relates_through_entry(&finding),
        "{label}: the test reaches `decodeThing` through an importer, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    // Indirect reach: never `exposed` however exact the oracle, unknown and
    // not an actionable gap.
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
    assert_eq!(
        finding.stop_reasons,
        [StopReason::PropagationEvidenceUnknown],
        "{label}"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("only through cross-module callers")),
        "{label}: missing: {:?}",
        finding.missing
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("repair_packet_ready: true")),
        "{label}: indirect reach never starts a repair: {:?}",
        finding.evidence
    );
    Ok(())
}

fn assert_no_static_path(label: &str, files: &Files<'_>) -> Result<(), String> {
    let finding = finding(label, files)?;
    assert!(
        !relates_through_entry(&finding),
        "{label}: no importer reach is established, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

/// The positive twin must relate and the variant must not: the variant's single
/// changed element is the refusal.
fn assert_refused(label: &str, variant: &Files<'_>) -> Result<(), String> {
    assert_related(&format!("{label}-twin"), &base())?;
    assert_parsed(
        label,
        variant,
        &[("src/a.ts", "decodeThing"), ("src/b.ts", "normalize")],
    );
    assert_no_static_path(label, variant)
}

#[test]
fn an_imported_helper_relates_through_the_exported_caller() -> Result<(), String> {
    let files = base();
    assert_parsed(
        "w1",
        &files,
        &[("src/a.ts", "decodeThing"), ("src/b.ts", "normalize")],
    );
    assert_eq!(
        cross_entries(&files, "src/a.ts", "decodeThing"),
        [("normalize".to_string(), 1, PathBuf::from("src/b.ts"))]
    );
    assert_related("w1", &files)?;
    // The summary names the importer export the test calls.
    let finding = finding("w1-summary", &files)?;
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("(`normalize`)")),
        "missing: {:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_barrel_import_of_the_exported_caller_relates() -> Result<(), String> {
    // w2: the test imports `normalize` from the package barrel; `b` still
    // imports the helper from `./a` directly.
    let files = with(
        with(
            base(),
            "src/index.ts",
            "export * from \"./a\";\nexport * from \"./b\";\n".to_string(),
        ),
        "tests/b.test.ts",
        B_TEST.replace("../src/b", "../src"),
    );
    assert_related("w2", &files)
}

#[test]
fn a_helper_imported_through_a_barrel_relates() -> Result<(), String> {
    let files = with(
        with(
            base(),
            "src/index.ts",
            "export * from \"./a\";\n".to_string(),
        ),
        "src/b.ts",
        B.replace("./a", "./index"),
    );
    assert_eq!(
        cross_entries(&files, "src/a.ts", "decodeThing"),
        [("normalize".to_string(), 1, PathBuf::from("src/b.ts"))]
    );
    assert_related("helper-via-barrel", &files)
}

#[test]
fn a_renamed_import_and_a_renamed_export_relate() -> Result<(), String> {
    let renamed_import = with(
        base(),
        "src/b.ts",
        "import { decodeThing as decode } from \"./a\";\nexport function normalize(p: string): string {\n  return \"/\" + decode(p);\n}\n"
            .to_string(),
    );
    assert_related("renamed-import", &renamed_import)?;
    let renamed_export = with(
        base(),
        "src/a.ts",
        format!(
            "function decodeThing(text: string): string {{\n{A_LINE}\n}}\nexport {{ decodeThing as decodeText }};\n"
        ),
    );
    let renamed_export = with(
        renamed_export,
        "src/b.ts",
        B.replace("{ decodeThing }", "{ decodeText }")
            .replace("decodeThing(p)", "decodeText(p)"),
    );
    assert_related("renamed-export", &renamed_export)
}

#[test]
fn a_default_import_of_the_helper_relates() -> Result<(), String> {
    let files = with(
        with(
            base(),
            "src/a.ts",
            A.replace("export function", "export default function"),
        ),
        "src/b.ts",
        B.replace("{ decodeThing }", "decodeThing"),
    );
    assert_related("default-import", &files)
}

#[test]
fn a_private_helper_of_the_imported_module_relates_through_its_export() -> Result<(), String> {
    // `normalize` -> `parseQuery` (import) -> `decodeThing` (same module):
    // two edges.
    let a = format!(
        "function decodeThing(text: string): string {{\n{A_LINE}\n}}\n\nexport function parseQuery(text: string): string {{\n  return decodeThing(text);\n}}\n"
    );
    let b = B
        .replace("{ decodeThing }", "{ parseQuery }")
        .replace("decodeThing(p)", "parseQuery(p)");
    let files = with(with(base(), "src/a.ts", a), "src/b.ts", b);
    assert_parsed(
        "private-helper",
        &files,
        &[("src/a.ts", "decodeThing"), ("src/b.ts", "normalize")],
    );
    assert_eq!(
        cross_entries(&files, "src/a.ts", "decodeThing"),
        [("normalize".to_string(), 2, PathBuf::from("src/b.ts"))]
    );
    assert_related("private-helper", &files)
}

#[test]
fn a_chain_across_three_modules_relates_within_the_bound() -> Result<(), String> {
    // test -> `entry` (c) -> `normalize` (b) -> `decodeThing` (a): two edges.
    let files = with(
        with(
            base(),
            "src/c.ts",
            "import { normalize } from \"./b\";\nexport function entry(p: string): string {\n  return normalize(p);\n}\n"
                .to_string(),
        ),
        "tests/b.test.ts",
        B_TEST
            .replace("../src/b", "../src/c")
            .replace("normalize", "entry"),
    );
    assert_eq!(
        cross_entries(&files, "src/a.ts", "decodeThing"),
        [
            ("entry".to_string(), 2, PathBuf::from("src/c.ts")),
            ("normalize".to_string(), 1, PathBuf::from("src/b.ts")),
        ]
    );
    assert_related("three-modules", &files)
}

#[test]
fn the_chain_is_bounded_to_three_call_edges() -> Result<(), String> {
    // normalize -> one -> two -> decodeThing is three edges: related.
    let three = "import { decodeThing } from \"./a\";\nfunction two(p: string): string {\n  return decodeThing(p);\n}\nfunction one(p: string): string {\n  return two(p);\n}\nexport function normalize(p: string): string {\n  return \"/\" + one(p);\n}\n".to_string();
    let files = with(base(), "src/b.ts", three.clone());
    assert_eq!(
        cross_entries(&files, "src/a.ts", "decodeThing"),
        [("normalize".to_string(), 3, PathBuf::from("src/b.ts"))]
    );
    assert_related("three-edges", &files)?;
    // One more same-module hop is four edges: refused.
    let four = three
        .replace(
            "function one(p: string): string {\n  return two(p);\n}",
            "function one(p: string): string {\n  return extra(p);\n}\nfunction extra(p: string): string {\n  return two(p);\n}",
        );
    let files = with(base(), "src/b.ts", four);
    assert!(cross_entries(&files, "src/a.ts", "decodeThing").is_empty());
    assert_refused("four-edges", &files)
}

#[test]
fn a_chain_longer_than_three_edges_across_modules_is_refused() -> Result<(), String> {
    // test -> entry (c) -> normalize (b) -> decodeThing (a) with `normalize`
    // reaching the helper through two private hops: 1 + 1 + 2 = four edges.
    let b = "import { decodeThing } from \"./a\";\nfunction two(p: string): string {\n  return decodeThing(p);\n}\nfunction one(p: string): string {\n  return two(p);\n}\nexport function normalize(p: string): string {\n  return one(p);\n}\n";
    let files = with(
        with(
            with(base(), "src/b.ts", b.to_string()),
            "src/c.ts",
            "import { normalize } from \"./b\";\nexport function entry(p: string): string {\n  return normalize(p);\n}\n"
                .to_string(),
        ),
        "tests/b.test.ts",
        B_TEST
            .replace("../src/b", "../src/c")
            .replace("normalize", "entry"),
    );
    let names: Vec<String> = cross_entries(&files, "src/a.ts", "decodeThing")
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    assert_eq!(names, ["normalize"], "`entry` is the fourth edge");
    assert_refused("long-chain", &files)
}

#[test]
fn a_shadowed_helper_name_is_refused() -> Result<(), String> {
    // A parameter, a local binding and an inner function each rebind the name.
    for (label, b) in [
        (
            "shadow-param",
            "import { decodeThing } from \"./a\";\nexport function normalize(p: string, decodeThing: (s: string) => string): string {\n  return \"/\" + decodeThing(p);\n}\n",
        ),
        (
            "shadow-local",
            "import { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  const decodeThing = (s: string) => s;\n  return \"/\" + decodeThing(p);\n}\n",
        ),
        (
            "shadow-inner-function",
            "import { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  function decodeThing(s: string) { return s; }\n  return \"/\" + decodeThing(p);\n}\n",
        ),
        (
            "shadow-callback-param",
            "import { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  return [p].map((decodeThing) => decodeThing(p)).join(\"\");\n}\n",
        ),
    ] {
        assert_refused(label, &with(base(), "src/b.ts", b.to_string()))?;
    }
    Ok(())
}

#[test]
fn a_same_named_helper_from_an_unrelated_module_is_refused() -> Result<(), String> {
    // `b` calls the `decodeThing` of `other`, not the changed one of `a`.
    let files = with(
        with(
            base(),
            "src/other.ts",
            "export function decodeThing(text: string): string {\n  return text;\n}\n".to_string(),
        ),
        "src/b.ts",
        B.replace("./a", "./other"),
    );
    assert!(cross_entries(&files, "src/a.ts", "decodeThing").is_empty());
    assert_refused("unrelated-module", &files)?;
    // The unrelated module's own helper IS reached through `normalize`.
    assert_eq!(
        cross_entries(&files, "src/other.ts", "decodeThing"),
        [("normalize".to_string(), 1, PathBuf::from("src/b.ts"))]
    );
    Ok(())
}

#[test]
fn an_ambiguous_star_export_fails_closed() -> Result<(), String> {
    // Two star sources export `decodeThing` with different bindings.
    let files = with(
        with(
            with(
                base(),
                "src/other.ts",
                "export function decodeThing(text: string): string {\n  return text;\n}\n"
                    .to_string(),
            ),
            "src/barrel.ts",
            "export * from \"./a\";\nexport * from \"./other\";\n".to_string(),
        ),
        "src/b.ts",
        B.replace("./a", "./barrel"),
    );
    assert!(cross_entries(&files, "src/a.ts", "decodeThing").is_empty());
    assert_refused("ambiguous-star", &files)
}

#[test]
fn a_mocked_module_on_the_chain_is_refused() -> Result<(), String> {
    // The importer's callee, the importer itself and a barrel in between each
    // replace the code the chain runs; the twin without the mock relates.
    let mocked_helper = B_TEST.replace("describe(", "vi.mock(\"../src/a\");\ndescribe(");
    assert_refused(
        "mock-helper-module",
        &with(base(), "tests/b.test.ts", mocked_helper),
    )?;
    let mocked_importer = B_TEST.replace("describe(", "vi.mock(\"../src/b\");\ndescribe(");
    assert_refused(
        "mock-importer-module",
        &with(base(), "tests/b.test.ts", mocked_importer),
    )?;
    let barrel = with(
        with(
            base(),
            "src/index.ts",
            "export * from \"./a\";\n".to_string(),
        ),
        "src/b.ts",
        B.replace("./a", "./index"),
    );
    assert_related("mock-barrel-twin", &barrel)?;
    let mocked_barrel = with(
        barrel,
        "tests/b.test.ts",
        B_TEST.replace("describe(", "vi.mock(\"../src/index\");\ndescribe("),
    );
    let finding = finding("mock-barrel", &mocked_barrel)?;
    assert!(
        !relates_through_entry(&finding),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn a_fabricating_spy_on_the_chain_is_refused() -> Result<(), String> {
    let spied = "import { describe, expect, test, vi } from \"vitest\";\nimport * as a from \"../src/a\";\nimport { normalize } from \"../src/b\";\ndescribe(\"normalize\", () => {\n  test(\"decodes\", () => {\n    vi.spyOn(a, \"decodeThing\").mockReturnValue(\"a/b\");\n    expect(normalize(\"a%252Fb\")).toBe(\"/a/b\");\n  });\n});\n";
    assert_refused("spy", &with(base(), "tests/b.test.ts", spied.to_string()))
}

#[test]
fn a_dynamic_or_computed_call_is_refused() -> Result<(), String> {
    for (label, body) in [
        (
            "alias-then-call",
            "  const fn = decodeThing;\n  return \"/\" + fn(p);",
        ),
        ("dot-call", "  return \"/\" + decodeThing.call(null, p);"),
        (
            "computed-lookup",
            "  return \"/\" + ({ decodeThing })[\"decodeThing\"](p);",
        ),
        (
            "passed-as-value",
            "  return [p].map(decodeThing).join(\"\");",
        ),
        (
            "reassigned",
            "  decodeThing = (s: string) => s;\n  return \"/\" + decodeThing(p);",
        ),
    ] {
        let b = format!(
            "import {{ decodeThing }} from \"./a\";\nexport function normalize(p: string): string {{\n{body}\n}}\n"
        );
        assert_refused(label, &with(base(), "src/b.ts", b))?;
    }
    Ok(())
}

#[test]
fn a_name_that_is_only_mentioned_is_refused() -> Result<(), String> {
    for (label, b) in [
        (
            "module-level-mention",
            "import { decodeThing } from \"./a\";\nexport const names = [decodeThing];\nexport function normalize(p: string): string {\n  return \"/\" + p;\n}\n",
        ),
        (
            "string-mention",
            "import { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  return \"/decodeThing\" + p;\n}\n",
        ),
        (
            "comment-mention",
            "import { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  // decodeThing is applied by the caller\n  return \"/\" + p;\n}\n",
        ),
    ] {
        assert_refused(label, &with(base(), "src/b.ts", b.to_string()))?;
    }
    Ok(())
}

#[test]
fn a_namespace_import_of_the_helper_module_is_not_followed() -> Result<(), String> {
    let b = "import * as a from \"./a\";\nexport function normalize(p: string): string {\n  return \"/\" + a.decodeThing(p);\n}\n";
    assert_refused("namespace-import", &with(base(), "src/b.ts", b.to_string()))
}

#[test]
fn a_helper_no_tested_export_reaches_is_refused() -> Result<(), String> {
    // The test calls an export of `b` that never reaches the helper; the
    // export that does has no test.
    let b = format!("{B}export function other(p: string): string {{\n  return p.trim();\n}}\n");
    let test = B_TEST
        .replace("{ normalize }", "{ other }")
        .replace("normalize(", "other(");
    let files = with(with(base(), "src/b.ts", b), "tests/b.test.ts", test);
    assert_parsed(
        "unreached",
        &files,
        &[("src/a.ts", "decodeThing"), ("src/b.ts", "other")],
    );
    assert_no_static_path("unreached", &files)
}

#[test]
fn an_importer_that_only_a_type_import_names_is_refused() -> Result<(), String> {
    let b = "import type { decodeThing } from \"./a\";\nexport function normalize(p: string): string {\n  return \"/\" + p;\n}\n";
    assert_refused("type-import", &with(base(), "src/b.ts", b.to_string()))
}

#[test]
fn a_direct_owner_test_keeps_its_own_verdict_beside_the_importer_test() -> Result<(), String> {
    // The direct test's weak oracle decides; the importer test's exact value
    // must not lend it strength (the entry reach is only admitted when no test
    // calls the owner itself).
    let direct = "import { describe, expect, test } from \"vitest\";\nimport { decodeThing } from \"../src/a\";\ndescribe(\"decodeThing\", () => {\n  test(\"runs\", () => {\n    expect(decodeThing(\"a\")).toBeDefined();\n  });\n});\n";
    let files = with(base(), "tests/a.test.ts", direct.to_string());
    let finding = finding("direct-beside-importer", &files)?;
    assert!(
        !relates_through_entry(&finding),
        "related: {:?}",
        finding.related_tests
    );
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn a_weak_oracle_behind_an_importer_is_unknown_not_an_actionable_gap() -> Result<(), String> {
    // Same-module entry reach with a weak oracle reads `weakly_exposed`;
    // reach two modules removed is unknown whatever the oracle.
    let weak = B_TEST.replace(
        "expect(normalize(\"a%252Fb\")).toBe(\"/a%2Fb\");",
        "expect(normalize(\"a%252Fb\")).toBeDefined();",
    );
    let files = with(base(), "tests/b.test.ts", weak);
    let finding = finding("weak-oracle", &files)?;
    assert!(
        relates_through_entry(&finding),
        "related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("repair_packet_ready: true")),
        "evidence: {:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn outbound_calls_are_persisted_with_the_file_facts() {
    // The workspace-level resolution never re-parses an importer: its
    // outbound calls ride in the cached export facts.
    let facts =
        extraction_cache::export_facts_cached(Path::new("src/b.ts"), B, Path::new("."), None)
            .unwrap_or_default();
    let calls: Vec<(String, Vec<String>)> = facts
        .outbound_calls
        .iter()
        .map(|call| {
            (
                call.import.local.clone(),
                call.entries
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        calls,
        [("decodeThing".to_string(), vec!["normalize".to_string()])]
    );
    let json = serde_json::to_string(&facts).unwrap_or_default();
    let round: ModuleExportFacts = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(round, facts);
}

/// `count` importer modules `src/mK.ts`, each exporting `getK` that calls
/// `decodeThing`, with one test per importer calling its `getK`.
fn fan_in(count: usize) -> Files<'static> {
    let mut files: Files<'static> = vec![("src/a.ts", A.to_string())];
    for k in 0..count {
        let module: &'static str = Box::leak(format!("src/m{k}.ts").into_boxed_str());
        let test: &'static str = Box::leak(format!("tests/m{k}.test.ts").into_boxed_str());
        files.push((
            module,
            format!(
                "import {{ decodeThing }} from \"./a\";\nexport function get{k}(p: string): string {{\n  return decodeThing(p) + \"{k}\";\n}}\n"
            ),
        ));
        files.push((
            test,
            format!(
                "import {{ expect, test }} from \"vitest\";\nimport {{ get{k} }} from \"../src/m{k}\";\ntest(\"get{k}\", () => {{\n  expect(get{k}(\"a%252Fb\")).toBe(\"a%2Fb{k}\");\n}});\n"
            ),
        ));
    }
    files
}

#[test]
fn fan_in_up_to_the_limit_relates_through_every_importer() -> Result<(), String> {
    let files = fan_in(MAX_CROSS_MODULE_ENTRIES);
    assert_parsed(
        "fan-in-at-limit",
        &files,
        &[("src/a.ts", "decodeThing"), ("src/m0.ts", "get0")],
    );
    assert_eq!(
        cross_entries(&files, "src/a.ts", "decodeThing").len(),
        MAX_CROSS_MODULE_ENTRIES
    );
    let finding = finding("fan-in-at-limit", &files)?;
    assert!(
        relates_through_entry(&finding),
        "related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    Ok(())
}

#[test]
fn fan_in_past_the_limit_follows_no_importer_and_says_so() -> Result<(), String> {
    // The twin one importer smaller relates (above); one more and no importer
    // is followed, so the relation scan never grows with the importer count.
    let files = fan_in(MAX_CROSS_MODULE_ENTRIES + 1);
    assert_parsed(
        "fan-in-past-limit",
        &files,
        &[("src/a.ts", "decodeThing"), ("src/m0.ts", "get0")],
    );
    assert!(cross_entries(&files, "src/a.ts", "decodeThing").is_empty());
    let finding = finding("fan-in-past-limit", &files)?;
    assert!(
        !relates_through_entry(&finding),
        "related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("past the limit ripr follows")),
        "the limit is disclosed: {:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn cross_module_reach_wording_names_the_other_module() -> Result<(), String> {
    let finding = finding("wording", &base())?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    let text = format!("{finding:?}");
    assert!(
        text.contains("an exported function of another module that imports and calls it"),
        "{text}"
    );
    assert!(
        !text.contains("only through a same-module caller"),
        "{text}"
    );
    Ok(())
}

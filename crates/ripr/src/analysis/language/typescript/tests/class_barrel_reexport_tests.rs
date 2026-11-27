//! Classes reached through a re-export barrel (RIPR-SPEC-0095, #13592 B3).
//! `import { Url } from "../src"` where `src/index.ts` has `export * from
//! "./u"` runs the same constructor and methods as `import { Url } from
//! "../src/u"`, so the receiver, constructor and static-call relations credit
//! it, disclosed as `re_export_chain_followed`. Anything that does not land on
//! the owner's own class (a same-named class of another module, an ambiguous
//! `export *`, a mocked module, a shadowing local, a class used only as a
//! type, a constructor the test does not name) stays uncredited.
//!
//! Every case builds the index through `ReExportIndex::build` over real
//! sources, applies the same import rewrite the analysis does, and relates
//! through `find_related_tests`, the production path. Each case first asserts
//! the parsed subjects (one test, one owner of the intended class), and every
//! negative has a credited twin differing only in the property under test.

use super::*;
use crate::domain::RelationReason;

const URL_CLASS: &str = "export class Url {\n  protocol: string;\n  constructor(input = \"\") {\n    this.protocol = input.split(\":\")[0];\n  }\n  get origin(): string {\n    return (this.protocol ? this.protocol + \"//\" : \"\") + \"host\";\n  }\n  static parse(input: string): string {\n    return input.trim();\n  }\n}\n";
const OTHER_URL_CLASS: &str =
    "export class Url {\n  get origin(): string {\n    return \"other\";\n  }\n}\n";
const DEFAULT_CLASS: &str =
    "export default class Url {\n  get origin(): string {\n    return \"host\";\n  }\n}\n";
const STAR: &str = "export * from \"./u\";\n";

fn test_source(import_line: &str, body: &str) -> String {
    format!(
        "import {{ expect, it, vi }} from \"vitest\";\n{import_line}\n\nit(\"origin\", () => {{\n{body}\n}});\n"
    )
}

const BARREL_IMPORT: &str = "import { Url } from \"../src\";";
const ORIGIN_BODY: &str =
    "  const url = new Url(\"https://x\");\n  expect(url.origin).toBe(\"https://host\");";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    /// The `origin` getter (a receiver-method owner).
    Origin,
    /// The constructor.
    Constructor,
    /// The static `parse` (a class-method owner).
    Parse,
}

/// The relation the single test in `tests/url.test.ts` has to `subject` of
/// the `Url` class declared in `src/u.ts`, or `None` when unrelated.
fn relation(
    files: &[(&str, &str)],
    source: &str,
    subject: Subject,
) -> Result<Option<RelationReason>, String> {
    let test_file = Path::new("tests/url.test.ts");
    let mut workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    workspace_files.push(test_file.to_path_buf());
    let mut sources = std::collections::HashMap::new();
    let mut owners = Vec::new();
    for (path, text) in files {
        sources.insert(PathBuf::from(path), (*text).to_string());
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
            owner.file == Path::new("src/u.ts")
                && owner.class_name.as_deref() == Some("Url")
                && match subject {
                    Subject::Origin => owner.name == "origin",
                    Subject::Constructor => owner.method_kind == TypeScriptMethodKind::Constructor,
                    Subject::Parse => owner.name == "parse",
                }
        })
        .collect();
    if wanted.len() != 1 {
        return Err(format!(
            "expected one {subject:?} owner of Url in src/u.ts, got {}",
            wanted.len()
        ));
    }
    let mut tests = extract_tests(test_file, source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    index.rewrite_class_barrel_imports(
        &mut tests,
        &related_tests::ExportedClasses::from_owners(&owners),
        None,
        None,
    );
    let related = find_related_tests(wanted[0], &tests, None, &index, None);
    Ok(related.first().and_then(|test| test.relation_reason))
}

fn files<'a>(barrel: &'a str, class: &'a str) -> [(&'a str, &'a str); 3] {
    [
        ("src/u.ts", class),
        ("src/index.ts", barrel),
        ("src/placeholder.ts", "export const placeholder = 1;\n"),
    ]
}

fn assert_credited(files: &[(&str, &str)], source: &str, subject: Subject, label: &str) {
    assert_eq!(
        relation(files, source, subject),
        Ok(Some(RelationReason::ReExportChainFollowed)),
        "{label}: a class reached through the barrel must relate, disclosed as the chain"
    );
}

fn assert_not_credited(files: &[(&str, &str)], source: &str, subject: Subject, label: &str) {
    let related = relation(files, source, subject);
    assert!(
        matches!(related, Ok(reason) if reason != Some(RelationReason::ReExportChainFollowed)),
        "{label}: must not be credited through the barrel, got {related:?}"
    );
}

#[test]
fn a_direct_import_control_is_not_a_barrel_hop() -> Result<(), String> {
    // The twin of every credited case below: the same body imported straight
    // from the declaring module relates directly, without the barrel hop.
    let source = test_source("import { Url } from \"../src/u\";", ORIGIN_BODY);
    let related = relation(&files(STAR, URL_CLASS), &source, Subject::Origin)?;
    assert_eq!(related, Some(RelationReason::DirectOwnerCall));
    Ok(())
}

#[test]
fn a_star_barrel_credits_the_receiver_method_constructor_and_static() -> Result<(), String> {
    let files = files(STAR, URL_CLASS);
    let source = test_source(BARREL_IMPORT, ORIGIN_BODY);
    assert_credited(&files, &source, Subject::Origin, "getter");
    assert_credited(&files, &source, Subject::Constructor, "constructor");
    let statics = test_source(BARREL_IMPORT, "  expect(Url.parse(\" a \")).toBe(\"a\");");
    assert_credited(&files, &statics, Subject::Parse, "static method");
    Ok(())
}

#[test]
fn named_renamed_nested_and_index_specifier_barrels_credit() -> Result<(), String> {
    let named = files("export { Url } from \"./u\";\n", URL_CLASS);
    assert_credited(
        &named,
        &test_source(BARREL_IMPORT, ORIGIN_BODY),
        Subject::Origin,
        "named re-export",
    );
    let renamed = files("export { Url as Link } from \"./u\";\n", URL_CLASS);
    let source = test_source(
        "import { Link } from \"../src\";",
        &ORIGIN_BODY.replace("Url", "Link"),
    );
    assert_credited(&renamed, &source, Subject::Origin, "renamed re-export");
    let aliased = test_source(
        "import { Url as U } from \"../src/index\";",
        &ORIGIN_BODY.replace("Url", "U"),
    );
    assert_credited(
        &files(STAR, URL_CLASS),
        &aliased,
        Subject::Origin,
        "import alias through an explicit index specifier",
    );
    let nested = [
        ("src/u.ts", URL_CLASS),
        ("src/inner.ts", STAR),
        ("src/index.ts", "export * from \"./inner\";\n"),
    ];
    assert_credited(
        &nested,
        &test_source(BARREL_IMPORT, ORIGIN_BODY),
        Subject::Origin,
        "two hops",
    );
    Ok(())
}

#[test]
fn a_default_exported_class_credits_through_a_default_re_export() -> Result<(), String> {
    let files = files("export { default as Url } from \"./u\";\n", DEFAULT_CLASS);
    assert_credited(
        &files,
        &test_source(BARREL_IMPORT, ORIGIN_BODY),
        Subject::Origin,
        "export { default as Url }",
    );
    Ok(())
}

#[test]
fn a_same_named_class_of_another_module_is_not_the_owner() -> Result<(), String> {
    let barrel = "export * from \"./other\";\n";
    let files = [
        ("src/u.ts", URL_CLASS),
        ("src/other.ts", OTHER_URL_CLASS),
        ("src/index.ts", barrel),
    ];
    let source = test_source(BARREL_IMPORT, ORIGIN_BODY);
    assert_not_credited(&files, &source, Subject::Origin, "other module's Url");
    assert_not_credited(&files, &source, Subject::Constructor, "other module's Url");
    // The twin: pointing the barrel at the owner's module credits.
    assert_credited(
        &[
            ("src/u.ts", URL_CLASS),
            ("src/other.ts", OTHER_URL_CLASS),
            ("src/index.ts", STAR),
        ],
        &source,
        Subject::Origin,
        "twin barrel of the owner module",
    );
    Ok(())
}

#[test]
fn an_ambiguous_export_star_is_not_followed() -> Result<(), String> {
    let ambiguous = [
        ("src/u.ts", URL_CLASS),
        ("src/other.ts", OTHER_URL_CLASS),
        (
            "src/index.ts",
            "export * from \"./u\";\nexport * from \"./other\";\n",
        ),
    ];
    let source = test_source(BARREL_IMPORT, ORIGIN_BODY);
    assert_not_credited(&ambiguous, &source, Subject::Origin, "two star sources");
    Ok(())
}

#[test]
fn a_mocked_barrel_or_class_module_is_not_followed() -> Result<(), String> {
    let files = files(STAR, URL_CLASS);
    for mocked in ["../src", "../src/u"] {
        let source = test_source(
            BARREL_IMPORT,
            &format!("  vi.mock(\"{mocked}\");\n{ORIGIN_BODY}"),
        );
        assert_not_credited(&files, &source, Subject::Origin, &format!("mock {mocked}"));
    }
    Ok(())
}

#[test]
fn a_shadowed_class_name_binds_nothing() -> Result<(), String> {
    let files = files(STAR, URL_CLASS);
    let local_class = format!("  class Url {{ get origin() {{ return \"x\"; }} }}\n{ORIGIN_BODY}");
    assert_not_credited(
        &files,
        &test_source(BARREL_IMPORT, &local_class),
        Subject::Origin,
        "body-local class Url",
    );
    let local_const = format!("  const Url = class {{}};\n{ORIGIN_BODY}");
    assert_not_credited(
        &files,
        &test_source(BARREL_IMPORT, &local_const),
        Subject::Origin,
        "body-local const Url",
    );
    Ok(())
}

#[test]
fn a_class_used_only_as_a_type_or_a_computed_constructor_is_not_constructed() -> Result<(), String>
{
    let files = files(STAR, URL_CLASS);
    let type_only = "  const url = makeUrl() as Url;\n  expect(url.origin).toBe(\"https://host\");";
    assert_not_credited(
        &files,
        &test_source("import type { Url } from \"../src\";", type_only),
        Subject::Origin,
        "type-only import",
    );
    assert_not_credited(
        &files,
        &test_source(BARREL_IMPORT, type_only),
        Subject::Origin,
        "type position only",
    );
    let computed =
        "  const url = new (pick())(\"https://x\");\n  expect(url.origin).toBe(\"https://host\");";
    assert_not_credited(
        &files,
        &test_source(BARREL_IMPORT, computed),
        Subject::Origin,
        "computed constructor",
    );
    let aliased = "  const C = Url;\n  const url = new C(\"https://x\");\n  expect(url.origin).toBe(\"https://host\");";
    assert_not_credited(
        &files,
        &test_source(BARREL_IMPORT, aliased),
        Subject::Origin,
        "constructor through a reassigned binding",
    );
    Ok(())
}

#[test]
fn another_export_of_the_barrel_or_a_namespace_import_is_not_the_class() -> Result<(), String> {
    let files = [
        ("src/u.ts", URL_CLASS),
        (
            "src/helpers.ts",
            "export function Url2(): number { return 1; }\n",
        ),
        (
            "src/index.ts",
            "export * from \"./u\";\nexport * from \"./helpers\";\n",
        ),
    ];
    let other = test_source(
        "import { Url2 } from \"../src\";",
        "  expect(Url2()).toBe(1);",
    );
    assert_not_credited(&files, &other, Subject::Origin, "a sibling export");
    let namespace = test_source(
        "import * as lib from \"../src\";",
        "  const url = new lib.Url(\"https://x\");\n  expect(url.origin).toBe(\"https://host\");",
    );
    assert_not_credited(
        &files,
        &namespace,
        Subject::Origin,
        "barrel namespace import",
    );
    Ok(())
}

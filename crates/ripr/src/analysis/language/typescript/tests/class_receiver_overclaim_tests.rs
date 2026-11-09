//! Class-receiver credit that must not over-claim (#13726).
//!
//! * A changed constructor is related to a test that constructs a class by
//!   bare name only when the name is unbound or bound to the owner's own
//!   class; a same-named class of another module that the test imports and
//!   constructs does not run the owner's constructor.
//! * A named-import alias (`import { Url as Link }`) constructs the owner's
//!   class and is credited in the owner's package, as it is across packages.
//! * A spied, mocked or replaced accessor is not the getter as written, so a
//!   read of it cannot notice a mutation of the getter body.
//!
//! Every case relates through `find_related_tests`, the production path, after
//! asserting the parsed subjects, and every negative has a credited twin that
//! differs only in the property under test.

use super::*;
use crate::domain::RelationReason;

const URL_CLASS: &str = "export class Url {\n  protocol: string;\n  constructor(input = \"\") {\n    this.protocol = input.split(\":\")[0];\n  }\n  get origin(): string {\n    return (this.protocol ? this.protocol + \"//\" : \"\") + \"host\";\n  }\n}\n";
const OTHER_URL_CLASS: &str = "export class Url {\n  constructor(input = \"\") {\n    this.protocol = input;\n  }\n  get origin(): string {\n    return \"other\";\n  }\n}\n";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Origin,
    Constructor,
}

fn test_source(import_line: &str, body: &str) -> String {
    format!(
        "import {{ expect, it, vi }} from \"vitest\";\n{import_line}\n\nit(\"origin\", () => {{\n{body}\n}});\n"
    )
}

fn relation(source: &str, subject: Subject) -> Result<Option<RelationReason>, String> {
    let test_file = Path::new("tests/url.test.ts");
    let files = [
        ("src/u.ts", URL_CLASS),
        ("src/other.ts", OTHER_URL_CLASS),
        ("src/placeholder.ts", "export const placeholder = 1;\n"),
    ];
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
            owner.file == Path::new("src/u.ts")
                && owner.class_name.as_deref() == Some("Url")
                && match subject {
                    Subject::Origin => owner.name == "origin",
                    Subject::Constructor => owner.method_kind == TypeScriptMethodKind::Constructor,
                }
        })
        .collect();
    if wanted.len() != 1 {
        return Err(format!(
            "expected one {subject:?} owner of Url in src/u.ts, got {}",
            wanted.len()
        ));
    }
    let tests = extract_tests(test_file, source);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    let related = find_related_tests(wanted[0], &tests, None, &index, None);
    Ok(related.first().and_then(|test| test.relation_reason))
}

fn assert_credited(source: &str, subject: Subject, label: &str) -> Result<(), String> {
    let related = relation(source, subject)?;
    if related.is_none() {
        return Err(format!("{label}: expected credit, got none"));
    }
    Ok(())
}

fn assert_withheld(source: &str, subject: Subject, label: &str) -> Result<(), String> {
    let related = relation(source, subject)?;
    if related.is_some() {
        return Err(format!("{label}: expected no relation, got {related:?}"));
    }
    Ok(())
}

const CTOR_BODY: &str =
    "  const url = new Url(\"https://x\");\n  expect(url.protocol).toBe(\"https\");";

#[test]
fn a_same_named_class_of_another_module_does_not_borrow_the_constructor() -> Result<(), String> {
    // Credited twin: the same body importing the owner's own module.
    assert_credited(
        &test_source("import { Url } from \"../src/u\";", CTOR_BODY),
        Subject::Constructor,
        "owner import",
    )?;
    assert_withheld(
        &test_source("import { Url } from \"../src/other\";", CTOR_BODY),
        Subject::Constructor,
        "other module's Url",
    )?;
    assert_withheld(
        &test_source("import Url from \"../src/other\";", CTOR_BODY),
        Subject::Constructor,
        "default import of another module under the class name",
    )?;
    assert_withheld(
        &test_source("import { Url } from \"node:url\";", CTOR_BODY),
        Subject::Constructor,
        "package import under the class name",
    )?;
    Ok(())
}

#[test]
fn an_unbound_class_name_keeps_the_bare_name_fallback() -> Result<(), String> {
    assert_credited(
        &test_source(
            "import { placeholder } from \"../src/placeholder\";",
            CTOR_BODY,
        ),
        Subject::Constructor,
        "unbound bare name",
    )
}

#[test]
fn a_named_import_alias_constructs_the_owner_class() -> Result<(), String> {
    let body = CTOR_BODY.replace("Url", "Link");
    assert_credited(
        &test_source("import { Url as Link } from \"../src/u\";", &body),
        Subject::Constructor,
        "aliased owner import",
    )?;
    assert_withheld(
        &test_source("import { Url as Link } from \"../src/other\";", &body),
        Subject::Constructor,
        "alias of another module's Url",
    )?;
    assert_withheld(
        &test_source("import { Other as Link } from \"../src/u\";", &body),
        Subject::Constructor,
        "alias of a different export",
    )?;
    Ok(())
}

const GETTER_BODY: &str =
    "  const url = new Url(\"https://x\");\n  expect(url.origin).toBe(\"https://host\");";

#[test]
fn a_spied_getter_withholds_observation_credit() -> Result<(), String> {
    let import = "import { Url } from \"../src/u\";";
    assert_credited(
        &test_source(import, GETTER_BODY),
        Subject::Origin,
        "unspied getter",
    )?;
    let spied = format!(
        "  vi.spyOn(Url.prototype, \"origin\", \"get\").mockReturnValue(\"x\");\n{}",
        GETTER_BODY.replace("https://host", "x")
    );
    assert_withheld(
        &test_source(import, &spied),
        Subject::Origin,
        "spied getter",
    )?;
    let call_through = format!("  vi.spyOn(Url.prototype, 'origin', 'get');\n{GETTER_BODY}");
    assert_withheld(
        &test_source(import, &call_through),
        Subject::Origin,
        "call-through accessor spy",
    )?;
    // A spy on an unrelated accessor of another name leaves the credit alone.
    let unrelated = format!("  vi.spyOn(Url.prototype, \"other\", \"get\");\n{GETTER_BODY}");
    assert_credited(
        &test_source(import, &unrelated),
        Subject::Origin,
        "spy on an unrelated accessor",
    )?;
    Ok(())
}

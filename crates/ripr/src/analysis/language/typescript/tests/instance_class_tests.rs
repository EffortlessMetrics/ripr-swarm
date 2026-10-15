//! RIPR-SPEC-0249 rule 1: `toBeInstanceOf(C)` reads exact for a predicate
//! change when the owner returns `C` on one path and a class that is never a
//! `C` on another.

use super::*;

const ERRORS: &str = "export class NotFound extends Error {}\nexport class Forbidden extends Error {}\n\nexport function errorFor(status: number): Error {\n  if (status === 404) {\n    return new NotFound(\"missing\");\n  }\n  return new Forbidden(\"denied\");\n}\n";

const OWNER: &str = "export function errorFor(status: number): Error {\n  if (status === 404) {\n    return new NotFound(\"missing\");\n  }\n  return new Forbidden(\"denied\");\n}";

const CHANGED: &str = "  if (status === 404) {";

#[test]
fn owner_branches_split_class_only_when_the_module_settles_them() {
    assert!(ts_owner_branches_split_class(
        OWNER, ERRORS, CHANGED, "NotFound"
    ));
    assert!(ts_owner_branches_split_class(
        OWNER,
        ERRORS,
        CHANGED,
        "Forbidden"
    ));
    // `instanceof` and a return type are plain uses of a class name.
    let reads = format!(
        "{ERRORS}export function isMissing(e: unknown): NotFound {{\n  if (e instanceof NotFound) {{\n    return e;\n  }}\n  return new NotFound(\"x\");\n}}\n"
    );
    assert!(ts_owner_branches_split_class(
        OWNER, &reads, CHANGED, "NotFound"
    ));
    // Comments and an import keep the module plain.
    let commented = format!(
        "import {{ audit }} from \"./audit\";\n// Errors for the HTTP layer.\n/* 404 and 403 */\n{ERRORS}"
    );
    assert!(ts_owner_branches_split_class(
        OWNER, &commented, CHANGED, "NotFound"
    ));
    // A plain constructor that only calls `super` is fine.
    let built = ERRORS.replace(
        "class NotFound extends Error {}",
        "class NotFound extends Error {\n  constructor(message: string) {\n    super(message);\n  }\n}",
    );
    assert!(ts_owner_branches_split_class(
        OWNER, &built, CHANGED, "NotFound"
    ));
    // A builtin on the other branch is never a user class instance.
    let builtin = OWNER.replace("new Forbidden(", "new RangeError(");
    assert!(ts_owner_branches_split_class(
        &builtin, ERRORS, CHANGED, "NotFound"
    ));
    for (label, owner, module, asserted) in [
        (
            "the other class extends the asserted one",
            OWNER.to_string(),
            ERRORS.replace("class Forbidden extends Error", "class Forbidden extends NotFound"),
            "NotFound",
        ),
        (
            "a module class extends the asserted one through another",
            OWNER.to_string(),
            ERRORS.replace("class Forbidden extends Error", "class Forbidden extends Gone")
                + "class Gone extends NotFound {}\n",
            "NotFound",
        ),
        (
            "a module class shadows a builtin name",
            OWNER.to_string(),
            ERRORS.replace("class Forbidden extends Error", "class Forbidden extends Map")
                + "class Map extends NotFound {}\n",
            "NotFound",
        ),
        (
            "both branches return the asserted class",
            OWNER.replace("new Forbidden(", "new NotFound("),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "the false branch falls through to another branch",
            OWNER.replace(
                "  return new Forbidden(\"denied\");",
                "  if (status >= 400) {\n    return new NotFound(\"client\");\n  }\n  return new Forbidden(\"denied\");",
            ),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "a branch returns something other than a new instance",
            OWNER.replace("return new Forbidden(\"denied\");", "return cached;"),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "a branch calls a method on the new instance",
            OWNER.replace("new Forbidden(\"denied\");", "new Forbidden(\"denied\").asNotFound();"),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "a branch holds another statement",
            OWNER.replace(
                "  return new Forbidden(\"denied\");",
                "  const h = { other() { return new Forbidden(\"x\"); } };\n  return new Forbidden(\"denied\");",
            ),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "the changed if sits in an arrow",
            "export function errorFor(status: number): Error {\n  const pick = () => {\n  if (status === 404) {\n    return new NotFound(\"missing\");\n  }\n  return new Forbidden(\"denied\");\n  };\n  audit(pick());\n  return new NotFound(\"x\");\n}".to_string(),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "the other class is imported",
            OWNER.replace("new Forbidden(", "new Remote("),
            ERRORS.to_string(),
            "NotFound",
        ),
        (
            "the asserted class is a builtin",
            OWNER.replace("new NotFound(", "new Error("),
            ERRORS.to_string(),
            "Error",
        ),
        (
            "the asserted class is not declared in the module",
            OWNER.to_string(),
            ERRORS.replace("class NotFound extends Error {}", ""),
            "NotFound",
        ),
        (
            "a mixin parent is not read",
            OWNER.to_string(),
            ERRORS.replace("class Forbidden extends Error", "class Forbidden extends mix(NotFound)"),
            "NotFound",
        ),
        (
            "a generic class header is not read",
            OWNER.to_string(),
            ERRORS.replace("class Forbidden extends Error", "class Forbidden<T> extends NotFound"),
            "NotFound",
        ),
        (
            "the other class's builtin parent is imported",
            OWNER.to_string(),
            format!("import {{ Error }} from \"./base\";\n{ERRORS}"),
            "NotFound",
        ),
        (
            "the module assigns the other class's builtin parent",
            OWNER.to_string(),
            format!("{ERRORS}Error = NotFound;\n"),
            "NotFound",
        ),
        (
            "the asserted class is nested and a non-class export takes its name",
            OWNER.to_string(),
            format!(
                "export const NotFound = Error;\nexport class Forbidden extends Error {{}}\nfunction scope() {{\n  class NotFound extends Error {{}}\n}}\n{OWNER}\n"
            ),
            "NotFound",
        ),
        (
            "the asserted class is not exported",
            OWNER.to_string(),
            ERRORS.replace("export class NotFound", "class NotFound"),
            "NotFound",
        ),
        (
            "an export rename takes the asserted name",
            OWNER.to_string(),
            ERRORS.replace("export class NotFound", "class NotFound")
                + "export { Forbidden as NotFound };\n",
            "NotFound",
        ),
        (
            "the asserted class extends an imported parent",
            OWNER.to_string(),
            format!(
                "import {{ Base }} from \"./base\";\n{}",
                ERRORS.replace("class NotFound extends Error", "class NotFound extends Base")
            ),
            "NotFound",
        ),
        (
            "a type assertion is not told apart from a rename",
            OWNER.to_string(),
            format!("{ERRORS}export function cast(e: unknown) {{\n  return e as NotFound;\n}}\n"),
            "NotFound",
        ),
        (
            "a parameter shadows the other class",
            OWNER.replace("(status: number)", "(status: number, Forbidden: typeof NotFound)"),
            ERRORS.replace("(status: number)", "(status: number, Forbidden: typeof NotFound)"),
            "NotFound",
        ),
        (
            "a logical assignment rebinds the other class",
            OWNER.to_string(),
            format!("{ERRORS}Forbidden &&= NotFound;\n"),
            "NotFound",
        ),
        (
            "a global object patches a builtin parent",
            OWNER.to_string(),
            format!("(globalThis as any).Error = NotFound;\n{ERRORS}"),
            "NotFound",
        ),
        (
            "a decorator can replace the other class",
            OWNER.to_string(),
            ERRORS.replace("export class Forbidden", "@swap export class Forbidden"),
            "NotFound",
        ),
        (
            "a string-keyed hook",
            OWNER.to_string(),
            ERRORS.replace(
                "class NotFound extends Error {}",
                "class NotFound extends Error {\n  static [Symbol[\"hasInstance\"]](x: unknown) {\n    return true;\n  }\n}",
            ),
            "NotFound",
        ),
        (
            "a string-named constructor",
            OWNER.to_string(),
            ERRORS.replace(
                "class Forbidden extends Error {}",
                "class Forbidden extends Error {\n  \"constructor\"() {\n    super();\n  }\n}",
            ),
            "NotFound",
        ),
        (
            "a static block installs a hook through a reflected symbol",
            OWNER.to_string(),
            format!(
                "const h = Object.getOwnPropertySymbols(Object.getPrototypeOf(Object))[0];\n{}",
                ERRORS.replace(
                    "class NotFound extends Error {}",
                    "class NotFound extends Error {\n  static {\n    Object.defineProperty(this, h, {{ value: () => true }});\n  }\n}",
                )
            ),
            "NotFound",
        ),
        (
            "a computed member rewrites a prototype",
            OWNER.to_string(),
            format!("let p: any;\nlet q: any;\n{ERRORS}x[\"set\" + \"PrototypeOf\"](q, p);\n"),
            "NotFound",
        ),
        (
            "a constructor reached through a member",
            OWNER.to_string(),
            format!("{ERRORS}const make = (1).constructor;\n"),
            "NotFound",
        ),
        (
            "a destructured constructor reaches Object",
            OWNER.to_string(),
            format!(
                "const {{ constructor: O }} = {{}};\nconst {{ getPrototypeOf: gp, defineProperty: dp }} = O;\n{}",
                ERRORS.replace(
                    "class NotFound extends Error {}",
                    "class NotFound extends Error {\n  install() {\n    const { constructor: K } = this;\n    dp(K, gp(O), {});\n  }\n}",
                )
            ),
            "NotFound",
        ),
        (
            "a computed destructuring key reaches Object",
            OWNER.to_string(),
            format!("const {{ [\"constr\" + \"uctor\"]: O }} = {{}};\n{ERRORS}"),
            "NotFound",
        ),
        (
            "a legacy accessor rewrites a prototype from a top-level statement",
            OWNER.to_string(),
            format!(
                "const {{ __lookupGetter__: lg, __lookupSetter__: ls }} = {{}};\nconst k = \"__pro\" + \"to__\";\nconst get = lg.call({{}}, k), set = ls.call({{}}, k);\nlet np: unknown;\n{ERRORS}"
            ),
            "NotFound",
        ),
        (
            "a method body reaches its class through this",
            OWNER.to_string(),
            ERRORS.replace(
                "class NotFound extends Error {}",
                "class NotFound extends Error {\n  grab() {\n    return this;\n  }\n}",
            ),
            "NotFound",
        ),
        (
            "an escaped string key",
            OWNER.to_string(),
            format!("{ERRORS}const {{ \"constr\\u0075ctor\": O }} = {{}};\n"),
            "NotFound",
        ),
        (
            "a function calls through a parameter",
            OWNER.to_string(),
            format!("{ERRORS}export function install(cb: Callback) {{\n  cb();\n}}\n"),
            "NotFound",
        ),
        (
            "a function body uses a local",
            OWNER.to_string(),
            format!("{ERRORS}export function install(a: number) {{\n  let b = a;\n  return b;\n}}\n"),
            "NotFound",
        ),
        (
            "the two lexers disagree on a quote after a word",
            OWNER.replace(
                "  if (status === 404) {",
                "  if (status) '='; Forbidden = NotFound; ''\n  if (status === 404) {",
            ),
            ERRORS.replace(
                "  if (status === 404) {",
                "  if (status) '='; Forbidden = NotFound; ''\n  if (status === 404) {",
            ),
            "NotFound",
        ),
        (
            "a parameter shadows the other class and is constructed",
            OWNER.replace("(status: number)", "(status: number, Forbidden: any)"),
            ERRORS.replace("(status: number)", "(status: number, Forbidden: any)"),
            "NotFound",
        ),
        (
            "a return type scan crosses a statement",
            OWNER.to_string(),
            format!("export function g(a: number): number;\nForbidden &&= NotFound;\nexport class X {{}}\n{ERRORS}"),
            "NotFound",
        ),
        (
            "an import without a semicolon swallows a statement",
            OWNER.to_string(),
            format!("import {{ a }} from \"./a\"\nForbidden &&= NotFound;\n{ERRORS}"),
            "NotFound",
        ),
        (
            "an import clause swallows a statement without semicolons",
            OWNER.to_string(),
            format!("import {{ from }} from \"./a\"\nNotFound &&= from\n\"y\";\n{ERRORS}"),
            "NotFound",
        ),
        (
            "a return type runs across lines",
            OWNER.to_string(),
            format!("export function g(a: number): number\nexport class X {{}}\n{ERRORS}"),
            "NotFound",
        ),
        (
            "a plain body increments",
            OWNER.to_string(),
            format!("{ERRORS}export function bump(a: number) {{\n  return a++;\n}}\n"),
            "NotFound",
        ),
        (
            "a destructured prototype",
            OWNER.to_string(),
            format!("{ERRORS}const {{ prototype: p }} = x;\n"),
            "NotFound",
        ),
        (
            "a namespace import reaches the module's own exports",
            OWNER.to_string(),
            format!("import * as me from \"./errors\";\n{ERRORS}"),
            "NotFound",
        ),
        (
            "a unicode escape spells the other class",
            OWNER.replace("(status: number)", "(status: number, \\u0046orbidden: any)"),
            ERRORS.replace("(status: number)", "(status: number, \\u0046orbidden: any)"),
            "NotFound",
        ),
        (
            "an ambient class declaration",
            OWNER.to_string(),
            ERRORS.replace("export class Forbidden", "declare class Forbidden"),
            "NotFound",
        ),
    ] {
        assert!(
            !ts_owner_branches_split_class(&owner, &module, CHANGED, asserted),
            "{label}"
        );
    }
    // A one-line `if (c) { return a; }` closes its own body, so the scan
    // would read the enclosing block's returns as its branches.
    let one_line = "export function errorFor(status: number, admin: boolean): Error {\n  if (!admin) {\n    if (status === 404) { return new NotFound(\"a\"); }\n    return new NotFound(\"b\");\n  }\n  return new Forbidden(\"x\");\n}";
    assert!(!ts_owner_branches_split_class(
        one_line,
        ERRORS,
        "    if (status === 404) { return new NotFound(\"a\"); }",
        "NotFound"
    ));
    // A failing `case` label jumps to the next label, not the next
    // statement, so the line after its body is not its false branch.
    let case_owner = "export function errorFor(k: number): Error {\n  switch (k) {\n    case 1: {\n      return new NotFound(\"a\");\n    }\n    return new Forbidden(\"b\");\n    default:\n      return new NotFound(\"c\");\n  }\n}";
    assert!(!ts_owner_branches_split_class(
        case_owner,
        ERRORS,
        "    case 1: {",
        "NotFound"
    ));
    // The changed text also sits in a multi-line template literal. Masking
    // blanks the template's line breaks, the line counts differ, and the
    // scan refuses rather than reading the real `if` the diff missed.
    let template_owner = OWNER.replacen(
        "{\n",
        "{\n  const doc = `\n  if (status === 404) {\n`;\n",
        1,
    );
    assert!(!ts_owner_branches_split_class(
        &template_owner,
        ERRORS,
        CHANGED,
        "NotFound"
    ));
}

fn errors_finding(
    label: &str,
    module: &str,
    test: &str,
    old: &str,
    new: &str,
    line: usize,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/errors.ts"), &module.replace(old, new))?;
    ts_write_file(&root.join("tests/errors.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/errors.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: new.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: old.to_string(),
            new_side_line: line,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn errors_test(body: &str) -> String {
    format!(
        "import {{ errorFor, Forbidden, NotFound }} from \"../src/errors\";\ntest(\"status codes\", async () => {{\n{body}\n}});\n"
    )
}

const PREDICATE_OLD: &str = "  if (status === 404) {";
const PREDICATE_NEW: &str = "  if (404 === status) {";

#[test]
fn instance_pin_exposes_a_predicate_whose_paths_return_different_classes() -> Result<(), String> {
    let pinned = errors_test(
        "  expect(errorFor(404)).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    let tests = extract_tests(Path::new("tests/errors.test.ts"), &pinned);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 2);
    assert_eq!(tests[0].assertions[0].matcher, "toBeInstanceOf");
    assert_eq!(
        tests[0].assertions[0].expected_value_or_variant.as_deref(),
        Some("NotFound")
    );
    assert_eq!(
        tests[0].assertions[0].oracle_kind,
        OracleKind::RelationalCheck
    );
    let finding = errors_finding(
        "instance-pin",
        ERRORS,
        &pinned,
        PREDICATE_OLD,
        PREDICATE_NEW,
        5,
    )?;
    assert_eq!(finding.probe.family, ProbeFamily::Predicate);
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let row = finding
        .related_tests
        .first()
        .ok_or("expected a related test row")?;
    assert_eq!(row.oracle_kind, OracleKind::ExactValue);
    assert_eq!(row.oracle_strength, OracleStrength::Strong);
    // A recognised matcher is not an unresolved custom matcher.
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("typescript_custom_matcher_unresolved")),
        "{:?}",
        finding.evidence
    );

    // A local bound once to the owner call is the whole result too.
    let local = errors_test(
        "  const missing = errorFor(404);\n  expect(missing).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    let finding = errors_finding(
        "instance-local",
        ERRORS,
        &local,
        PREDICATE_OLD,
        PREDICATE_NEW,
        5,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn instance_pin_stays_weak_without_a_settled_class_split() -> Result<(), String> {
    let pinned = errors_test(
        "  expect(errorFor(404)).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    let subclass = ERRORS.replace(
        "class Forbidden extends Error",
        "class Forbidden extends NotFound",
    );
    // `instanceof` consults a `Symbol.hasInstance` hook, and a constructor
    // can return another object, so the `extends` chain no longer decides.
    let hooked = ERRORS.replace(
        "class NotFound extends Error {}",
        "class NotFound extends Error {\n  static [Symbol.hasInstance](v: unknown) {\n    return v instanceof Error;\n  }\n}",
    );
    let returning = ERRORS.replace(
        "class Forbidden extends Error {}",
        "class Forbidden extends Error {\n  constructor(m: string) {\n    super(m);\n    return new NotFound(m);\n  }\n}",
    );
    let other_module = "import { errorFor } from \"../src/errors\";\nimport { NotFound } from \"../src/other\";\ntest(\"status codes\", () => {\n  expect(errorFor(404)).toBeInstanceOf(NotFound);\n});\n".to_string();
    for (label, module, test) in [
        ("instance-subclass", subclass.as_str(), pinned.clone()),
        ("instance-has-instance", hooked.as_str(), pinned.clone()),
        (
            "instance-returning-constructor",
            returning.as_str(),
            pinned.clone(),
        ),
        (
            "instance-shadowed-class",
            ERRORS,
            errors_test(
                "  const NotFound = Error;\n  expect(errorFor(404)).toBeInstanceOf(NotFound);",
            ),
        ),
        ("instance-other-module-class", ERRORS, other_module),
        (
            "instance-not-owner-result",
            ERRORS,
            errors_test(
                "  errorFor(404);\n  expect(new NotFound(\"x\")).toBeInstanceOf(NotFound);\n  expect(new Forbidden(\"y\")).toBeInstanceOf(Forbidden);",
            ),
        ),
        (
            "instance-wrapped-owner-call",
            ERRORS,
            errors_test(
                "  const normalize = (e: Error) => new NotFound(\"x\");\n  expect(normalize(errorFor(404))).toBeInstanceOf(NotFound);\n  expect(normalize(errorFor(403))).toBeInstanceOf(NotFound);",
            ),
        ),
        (
            "instance-rejects",
            ERRORS,
            errors_test(
                "  await expect(errorFor(404)).rejects.toBeInstanceOf(NotFound);\n  await expect(errorFor(403)).rejects.toBeInstanceOf(Forbidden);",
            ),
        ),
        (
            "instance-builtin",
            ERRORS,
            errors_test(
                "  expect(errorFor(404)).toBeInstanceOf(Error);\n  expect(errorFor(403)).toBeInstanceOf(Error);",
            ),
        ),
    ] {
        let line = module
            .lines()
            .position(|text| text == PREDICATE_OLD)
            .ok_or("changed line missing")?
            + 1;
        let finding = errors_finding(label, module, &test, PREDICATE_OLD, PREDICATE_NEW, line)?;
        assert_eq!(finding.probe.family, ProbeFamily::Predicate, "{label}");
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// Rule 1 reads predicate changes only: a message change on a return keeps
/// the class, so the pin passes on both versions.
#[test]
fn instance_pin_does_not_credit_a_returned_message_change() -> Result<(), String> {
    let pinned = errors_test(
        "  expect(errorFor(404)).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    let finding = errors_finding(
        "instance-message",
        ERRORS,
        &pinned,
        "  return new Forbidden(\"denied\");",
        "  return new Forbidden(\"refused\");",
        8,
    )?;
    assert_ne!(finding.probe.family, ProbeFamily::Predicate);
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// The facts are read for predicate changes only; on a value seam the same
/// assertions stay at their relational / weak reading.
#[test]
fn instance_facts_read_predicate_changes_only() -> Result<(), String> {
    let root = ts_unique_tempdir("instance-facts-family")?;
    ts_write_file(&root.join("src/errors.ts"), ERRORS)?;
    let test = errors_test(
        "  expect(errorFor(404)).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    ts_write_file(&root.join("tests/errors.test.ts"), &test)?;
    let owner = extract_owners(Path::new("src/errors.ts"), ERRORS)
        .into_iter()
        .find(|owner| owner.name == "errorFor")
        .ok_or("owner errorFor not extracted")?;
    let tests = extract_tests(Path::new("tests/errors.test.ts"), &test);
    let candidates =
        related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let read = |family: ProbeFamily| {
        let facts =
            ts_instance_class_facts(&family, CHANGED, &owner, &candidates, None, Some(&root));
        tests[0]
            .assertions
            .iter()
            .filter(|assertion| facts.credits(&tests[0].file, assertion))
            .count()
    };
    let predicate = read(ProbeFamily::Predicate);
    let value = read(ProbeFamily::ReturnValue);
    assert_eq!(predicate, 2);
    assert_eq!(value, 0);
    // Pins that do not observe the whole owner result, that reject, or that
    // share a line with a credited pin read no credit of their own.
    for (label, body, credited) in [
        (
            "not-owner-result",
            "  errorFor(404);\n  expect(new NotFound(\"x\")).toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "wrapped",
            "  const wrap = (e: Error) => e;\n  expect(wrap(errorFor(404))).toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "rejects",
            "  await expect(errorFor(404)).rejects.toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "same-line",
            "  expect(errorFor(403)).toBeInstanceOf(Forbidden); expect(wrap(errorFor(404))).toBeInstanceOf(NotFound);",
            1,
        ),
        (
            "resolves",
            "  await expect(errorFor(404)).resolves.toBeInstanceOf(NotFound);",
            1,
        ),
        (
            "shadowed-owner",
            "  const errorFor = (s: number) => new NotFound(\"x\");\n  expect(errorFor(404)).toBeInstanceOf(NotFound);",
            0,
        ),
    ] {
        let test = errors_test(body);
        let tests = extract_tests(Path::new("tests/errors.test.ts"), &test);
        let candidates =
            related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
        let facts = ts_instance_class_facts(
            &ProbeFamily::Predicate,
            CHANGED,
            &owner,
            &candidates,
            None,
            Some(&root),
        );
        let count = tests
            .iter()
            .flat_map(|test| test.assertions.iter().map(move |a| (test, a)))
            .filter(|(test, assertion)| facts.credits(&test.file, assertion))
            .count();
        assert_eq!(count, credited, "{label}");
    }
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

fn errors_test_with(prelude: &str, body: &str) -> String {
    format!(
        "import {{ errorFor, Forbidden, NotFound }} from \"../src/errors\";\n{prelude}\ntest(\"status codes\", async () => {{\n{body}\n}});\n"
    )
}

const IDENTITY_WRAP: &str = "function wrap(err: Error): Error {\n  return err;\n}";

/// RIPR-SPEC-0249 example 5 (amended 2026-10-15): a pin on
/// `wrap(errorFor(404))` reads as a pin on `errorFor(404)` when `wrap` is a
/// test-local identity helper; any other helper, call shape or binding keeps
/// the wrapped pin uncredited.
#[test]
fn instance_pin_looks_through_a_test_local_identity_helper() -> Result<(), String> {
    let wrapped = errors_test_with(
        IDENTITY_WRAP,
        "  expect(wrap(errorFor(404))).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    let tests = extract_tests(Path::new("tests/errors.test.ts"), &wrapped);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].identity_helpers_in_file, vec!["wrap".to_string()]);
    assert_eq!(
        tests[0].assertions[0].observed_expression.as_deref(),
        Some("wrap(errorFor(404))")
    );
    let finding = errors_finding(
        "instance-identity-wrap",
        ERRORS,
        &wrapped,
        PREDICATE_OLD,
        PREDICATE_NEW,
        5,
    )?;
    assert_eq!(finding.probe.family, ProbeFamily::Predicate);
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let row = finding
        .related_tests
        .first()
        .ok_or("expected a related test row")?;
    assert_eq!(row.oracle_kind, OracleKind::ExactValue);
    assert_eq!(row.oracle_strength, OracleStrength::Strong);

    // The same file with a helper that replaces its argument: the boundary
    // pin is not credited and the finding stays below `exposed`.
    let transforming = errors_test_with(
        "function wrap(err: Error): Error {\n  return new NotFound(\"x\");\n}",
        "  expect(wrap(errorFor(404))).toBeInstanceOf(NotFound);\n  expect(errorFor(403)).toBeInstanceOf(Forbidden);",
    );
    let finding = errors_finding(
        "instance-transforming-wrap",
        ERRORS,
        &transforming,
        PREDICATE_OLD,
        PREDICATE_NEW,
        5,
    )?;
    assert_eq!(finding.probe.family, ProbeFamily::Predicate);
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.missing
    );

    let root = ts_unique_tempdir("instance-identity-facts")?;
    ts_write_file(&root.join("src/errors.ts"), ERRORS)?;
    let owner = extract_owners(Path::new("src/errors.ts"), ERRORS)
        .into_iter()
        .find(|owner| owner.name == "errorFor")
        .ok_or("owner errorFor not extracted")?;
    let pin = "  expect(wrap(errorFor(404))).toBeInstanceOf(NotFound);";
    for (label, prelude, body, credited) in [
        ("identity-function", IDENTITY_WRAP, pin, 1),
        (
            "identity-arrow",
            "const wrap = (err: Error) => err;",
            pin,
            1,
        ),
        (
            "awaited-identity",
            IDENTITY_WRAP,
            "  expect(await wrap(errorFor(404))).toBeInstanceOf(NotFound);",
            1,
        ),
        (
            "transforming-helper",
            "function wrap(err: Error): Error {\n  return new NotFound(\"x\");\n}",
            pin,
            0,
        ),
        (
            "reassigned-helper",
            "function wrap(err: Error): Error {\n  return err;\n}\nwrap = (err: Error) => new NotFound(\"x\");",
            pin,
            0,
        ),
        (
            "shadowed-helper",
            IDENTITY_WRAP,
            "  const wrap = (err: Error) => new NotFound(\"x\");\n  expect(wrap(errorFor(404))).toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "exported-helper",
            "export function wrap(err: Error): Error {\n  return err;\n}",
            pin,
            0,
        ),
        (
            "two-arguments",
            IDENTITY_WRAP,
            "  expect(wrap(errorFor(404), 1)).toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "wrapped-local",
            IDENTITY_WRAP,
            "  const missing = errorFor(404);\n  expect(wrap(missing)).toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "wrapped-member",
            IDENTITY_WRAP,
            "  expect(wrap(errorFor(404).cause)).toBeInstanceOf(NotFound);",
            0,
        ),
        (
            "nested-wrap",
            IDENTITY_WRAP,
            "  expect(wrap(wrap(errorFor(404)))).toBeInstanceOf(NotFound);",
            0,
        ),
        ("undeclared-wrap", "", pin, 0),
    ] {
        let test = errors_test_with(prelude, body);
        let tests = extract_tests(Path::new("tests/errors.test.ts"), &test);
        assert_eq!(tests.len(), 1, "{label}");
        assert_eq!(tests[0].assertions.len(), 1, "{label}");
        // The call-shape cases keep the helper, so their refusal comes from
        // the classifier's call-site check, not from extraction.
        let helper_kept = matches!(
            label,
            "identity-function"
                | "identity-arrow"
                | "awaited-identity"
                | "two-arguments"
                | "wrapped-local"
                | "wrapped-member"
                | "nested-wrap"
        );
        assert_eq!(
            tests[0].identity_helpers_in_file.is_empty(),
            !helper_kept,
            "{label}"
        );
        let candidates =
            related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
        let facts = ts_instance_class_facts(
            &ProbeFamily::Predicate,
            CHANGED,
            &owner,
            &candidates,
            None,
            Some(&root),
        );
        let count = tests[0]
            .assertions
            .iter()
            .filter(|assertion| facts.credits(&tests[0].file, assertion))
            .count();
        assert_eq!(count, credited, "{label}");
    }
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

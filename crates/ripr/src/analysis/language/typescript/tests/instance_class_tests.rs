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
    ] {
        assert!(
            !ts_owner_branches_split_class(&owner, &module, CHANGED, asserted),
            "{label}"
        );
    }
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
    for (label, module, test) in [
        ("instance-subclass", subclass.as_str(), pinned.clone()),
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
        let finding = errors_finding(label, module, &test, PREDICATE_OLD, PREDICATE_NEW, 5)?;
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

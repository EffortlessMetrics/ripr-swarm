//! A raised class swapped for an imported one is not an imported call to
//! follow (#15356). The edit reads as a weak error-path gap naming the new
//! class; no superclass or class-only context credits it as exposed.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding, StaticLimitKind};
use std::path::Path;

type Check = Result<(), String>;

const SOURCE: &str = r#"from verlib.errors import InvalidVersion


def parse_release(text):
    parts = text.strip().split(".")
    if not all(part.isdigit() for part in parts):
        raise InvalidVersion(f"invalid version: {text!r}")
    return tuple(int(part) for part in parts)
"#;
const OLD: &str = r#"        raise ValueError(f"invalid version: {text!r}")"#;
const NEW: &str = r#"        raise InvalidVersion(f"invalid version: {text!r}")"#;

fn tests_with(expected: &str) -> String {
    format!(
        "import pytest\nfrom verlib.errors import InvalidVersion\nfrom verlib.version import parse_release\n\n\ndef test_rejects():\n    with pytest.raises({expected}):\n        parse_release(\"1.x\")\n"
    )
}

fn classify(source: &str, tests: &str, old: Option<&str>, new: &str) -> Result<Finding, String> {
    let file = Path::new("src/verlib/version.py");
    let owners = extract_owners(file, source);
    let tests = extract_tests(Path::new("tests/test_version.py"), tests);
    if !owners.iter().any(|owner| owner.name == "parse_release") || tests.is_empty() {
        return Err("the fixture must extract its owner and a test".to_string());
    }
    let line = source
        .lines()
        .position(|text| text == new)
        .map(|index| index + 1)
        .ok_or_else(|| "fixture has no changed line".to_string())?;
    classify_change_with_context(
        file,
        line,
        new,
        old,
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: true,
            module_import_unshadowed: true,
            pytest_import_unshadowed: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(source),
    )
    .ok_or_else(|| "the changed raise must produce a finding".to_string())
}

fn assert_gap(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None, "{finding:?}");
    assert!(
        finding
            .canonical_gap
            .as_ref()
            .is_some_and(|gap| gap.id.contains("invalidversion")),
        "the gap must name the new class: {finding:?}"
    );
}

fn assert_import_limit(label: &str, finding: &Finding) {
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MissingImportGraph),
        "{label}: {finding:?}"
    );
}

#[test]
fn imported_subclass_swap_reads_as_a_gap_not_an_import_limit() -> Check {
    assert_gap(&classify(
        SOURCE,
        &tests_with("ValueError"),
        Some(OLD),
        NEW,
    )?);
    Ok(())
}

#[test]
fn imported_subclass_swap_is_never_credited_exposed() -> Check {
    // The superclass context cannot tell the subclass from the old raise, and
    // a class-only context naming the new class is not read as exact either.
    for expected in ["ValueError", "Exception", "InvalidVersion"] {
        let finding = classify(SOURCE, &tests_with(expected), Some(OLD), NEW)?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{expected}: {finding:?}"
        );
        assert_gap(&finding);
    }
    Ok(())
}

#[test]
fn imported_raise_swap_guard_declines_other_shapes() -> Check {
    let tests = tests_with("ValueError");
    // Different arguments are a message edit, not a type swap.
    let message = r#"        raise InvalidVersion(f"bad version: {text!r}")"#;
    let edited = SOURCE.replace(NEW, message);
    assert_import_limit("message", &classify(&edited, &tests, Some(OLD), message)?);
    // A `snake_case` factory may build any exception.
    let factory = r#"        raise invalid_version(f"invalid version: {text!r}")"#;
    let factory_source = SOURCE
        .replace(NEW, factory)
        .replace("import InvalidVersion", "import invalid_version");
    assert_import_limit(
        "factory",
        &classify(&factory_source, &tests, Some(OLD), factory)?,
    );
    // No previous line to compare, or a previous non-raise line.
    assert_import_limit("no_old", &classify(SOURCE, &tests, None, NEW)?);
    assert_import_limit(
        "non_raise",
        &classify(
            SOURCE,
            &tests,
            Some(r#"        return InvalidVersion(f"invalid version: {text!r}")"#),
            NEW,
        )?,
    );
    // The class is not imported by a plain `from M import Name`.
    let aliased = SOURCE.replace("import InvalidVersion", "import Other as InvalidVersion");
    assert_import_limit("aliased", &classify(&aliased, &tests, Some(OLD), NEW)?);
    Ok(())
}

fn matching_tests(imports: &str, expected: &str) -> String {
    format!(
        "import pytest\n{imports}\n\ndef test_rejects():\n    with pytest.raises({expected}, match=\"invalid version\"):\n        parse_release(\"1.x\")\n"
    )
}

const IMPORT_RELEASE: &str = "from verlib.version import parse_release\n";

#[test]
fn match_pattern_on_a_base_class_does_not_credit_the_class_swap() -> Check {
    for expected in ["ValueError", "Exception", "(ValueError, InvalidVersion)"] {
        let tests = matching_tests(
            &format!("from verlib.errors import InvalidVersion\n{IMPORT_RELEASE}"),
            expected,
        );
        let finding = classify(SOURCE, &tests, Some(OLD), NEW)?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{expected}: {finding:?}"
        );
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{expected}");
    }
    Ok(())
}

#[test]
fn match_pattern_naming_the_new_imported_class_stays_exposed() -> Check {
    let tests = matching_tests(
        &format!("from verlib.errors import InvalidVersion\n{IMPORT_RELEASE}"),
        "InvalidVersion",
    );
    let finding = classify(SOURCE, &tests, Some(OLD), NEW)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // The same name bound from another module is not the owner's class.
    let other = matching_tests(
        &format!("from verlib.other import InvalidVersion\n{IMPORT_RELEASE}"),
        "InvalidVersion",
    );
    let finding = classify(SOURCE, &other, Some(OLD), NEW)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // An aliased import of another class is not read either.
    let aliased = matching_tests(
        &format!("from verlib.errors import Other as InvalidVersion\n{IMPORT_RELEASE}"),
        "InvalidVersion",
    );
    let finding = classify(SOURCE, &aliased, Some(OLD), NEW)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn match_pattern_on_the_old_builtin_does_not_credit_a_builtin_swap() -> Check {
    let source = "def parse_release(text):\n    parts = text.strip().split(\".\")\n    if not all(part.isdigit() for part in parts):\n        raise KeyError(\"invalid version\")\n    return tuple(int(part) for part in parts)\n";
    let old = "        raise ValueError(\"invalid version\")";
    let new = "        raise KeyError(\"invalid version\")";
    // An old-class expectation (`ValueError`) fails against the unrelated
    // `KeyError`, which the edit does notice; only base classes pass both.
    for expected in ["Exception", "LookupError"] {
        let tests = matching_tests(IMPORT_RELEASE, expected);
        let finding = classify(source, &tests, Some(old), new)?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{expected}: {finding:?}"
        );
    }
    let tests = matching_tests(IMPORT_RELEASE, "KeyError");
    let finding = classify(source, &tests, Some(old), new)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn match_pattern_on_a_base_class_does_not_credit_a_local_subclass_swap() -> Check {
    let source = "class InvalidVersion(ValueError):\n    pass\n\n\ndef parse_release(text):\n    parts = text.strip().split(\".\")\n    if not all(part.isdigit() for part in parts):\n        raise InvalidVersion(\"invalid version\")\n    return tuple(int(part) for part in parts)\n";
    let old = "        raise ValueError(\"invalid version\")";
    let new = "        raise InvalidVersion(\"invalid version\")";
    let imports = format!("from verlib.version import InvalidVersion, parse_release\n");
    let finding = classify(
        source,
        &matching_tests(&imports, "ValueError"),
        Some(old),
        new,
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    let finding = classify(
        source,
        &matching_tests(&imports, "InvalidVersion"),
        Some(old),
        new,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

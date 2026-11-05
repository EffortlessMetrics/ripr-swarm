//! Finding-specific exception classes must distinguish the actual edited raise.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding, OracleKind, OracleStrength, StageState};
use std::path::Path;

type Check = Result<(), String>;

const SOURCE: &str = "class EmptyError(Exception): pass\nclass TooLongError(Exception): pass\n\ndef parse(s):\n    if not s:\n        raise EmptyError()\n    if len(s) > 10:\n        raise TooLongError()\n    return len(s)\n";
const OLD: &str = "        raise EmptyError()";
const NEW: &str = "        raise TooLongError()";
const IMPORTS: &str = "import pytest\nfrom fx.core import parse, EmptyError, TooLongError\n";

fn test_source(expected: &str, argument: &str) -> String {
    format!(
        "{IMPORTS}\ndef test_variant():\n    with pytest.raises({expected}):\n        parse({argument})\n"
    )
}

fn classify(source: &str, tests: &str, old: &str, new: &str) -> Result<Finding, String> {
    classify_with_context(source, tests, Some(old), new, true)
}

fn classify_with_context(
    source: &str,
    tests: &str,
    old: Option<&str>,
    new: &str,
    sole: bool,
) -> Result<Finding, String> {
    let file = Path::new("src/fx/core.py");
    let owners = extract_owners(file, source);
    let tests = extract_tests(Path::new("tests/test_core.py"), tests);
    if !owners.iter().any(|owner| owner.name == "parse") || tests.is_empty() {
        return Err("the fixture must extract parse and at least one test".to_string());
    }
    let line = source
        .lines()
        .enumerate()
        .filter_map(|(index, text)| (text == new).then_some(index + 1))
        .last()
        .ok_or_else(|| "fixture has no changed raise line".to_string())?;
    classify_change_with_context(
        file,
        line,
        new,
        old,
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: sole,
            pytest_import_unshadowed: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(source),
    )
    .ok_or_else(|| "the changed raise must produce a finding".to_string())
}

fn assert_credited(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
    assert_eq!(
        finding.alignment_reason.as_deref(),
        Some("strong_oracle_observes_changed_exception_variant")
    );
    assert!(
        finding.observed_sink.as_deref().is_some_and(|sink| {
            sink.contains("pytest.raises(TooLongError)") && sink.contains("parse(")
        }),
        "the credited context must retain its own call: {finding:?}"
    );
    assert!(
        finding.evidence.iter().any(|evidence| {
            evidence.starts_with("exception_variant_read:")
                && evidence.contains("rejects `EmptyError` and accepts `TooLongError`")
        }),
        "{finding:?}"
    );
    assert!(
        finding
            .evidence
            .iter()
            .all(|evidence| !evidence.starts_with("suggested_repair_action:")),
        "an exact variant must not invite repair: {finding:?}"
    );
}

fn assert_no_credit(finding: &Finding) {
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_ne!(
        finding.alignment_reason.as_deref(),
        Some("strong_oracle_observes_changed_exception_variant"),
        "{finding:?}"
    );
    assert!(
        finding
            .evidence
            .iter()
            .all(|evidence| !evidence.starts_with("exception_variant_read:")),
        "{finding:?}"
    );
}

#[test]
fn specific_exception_variant_credits_the_changed_raise() -> Check {
    for argument in ["'abcdefghijklmnop'", "'ééééééééééé'"] {
        let finding = classify(SOURCE, &test_source("TooLongError", argument), OLD, NEW)?;
        assert_credited(&finding);
        let row = finding
            .related_tests
            .first()
            .ok_or_else(|| "credited finding needs its related test".to_string())?;
        assert_eq!(row.oracle.as_deref(), Some("pytest.raises(TooLongError)"));
        assert_eq!(row.oracle_kind, OracleKind::ExactErrorVariant);
        assert_eq!(row.oracle_strength, OracleStrength::Strong);
    }
    // An unchanged literal message has no effect on the class discriminator.
    let source = SOURCE.replace(NEW, "        raise TooLongError('too long')");
    assert_credited(&classify(
        &source,
        &test_source("TooLongError", "'abcdefghijklmnop'"),
        "        raise EmptyError('too long')",
        "        raise TooLongError('too long')",
    )?);
    Ok(())
}

#[test]
fn specific_exception_variant_preserves_the_selected_context() -> Check {
    let target = "    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n";
    let other = "    with pytest.raises(ValueError, match='other problem'):\n        raise ValueError('other problem')\n";
    for body in [
        format!("{target}{other}"),
        format!("    assert 2 + 3 == 5\n{target}{other}"),
        format!("{target}    assert 4 + 3 == 7\n"),
    ] {
        let tests = format!("{IMPORTS}\ndef test_variant():\n{body}");
        let finding = classify(SOURCE, &tests, OLD, NEW)?;
        assert_credited(&finding);
        assert_eq!(
            finding
                .related_tests
                .first()
                .and_then(|test| test.oracle.as_deref()),
            Some("pytest.raises(TooLongError)")
        );
        assert!(
            finding
                .observed_sink
                .as_deref()
                .is_some_and(|sink| !sink.contains("other problem")),
            "another context cannot donate its call: {finding:?}"
        );
    }
    // Each test row keeps its own grade even when another row credits the finding.
    let tests = format!(
        "{IMPORTS}\ndef test_variant():\n{target}\ndef test_broad():\n    with pytest.raises(Exception):\n        parse('abcdefghijklmnop')\n"
    );
    let finding = classify(SOURCE, &tests, OLD, NEW)?;
    assert_credited(&finding);
    let broad = finding
        .related_tests
        .iter()
        .find(|test| test.name == "test_broad")
        .ok_or_else(|| "the broad control needs its own row".to_string())?;
    assert_eq!(broad.oracle_kind, OracleKind::BroadError);
    assert_eq!(broad.oracle_strength, OracleStrength::Weak);
    Ok(())
}

#[test]
fn specific_exception_variant_declines_non_discriminating_types() -> Check {
    for expected in ["Exception", "EmptyError", "(TooLongError, Exception)"] {
        assert_no_credit(&classify(
            SOURCE,
            &test_source(expected, "'abcdefghijklmnop'"),
            OLD,
            NEW,
        )?);
    }
    // pytest accepts subclasses: distinct names alone are not a discriminator.
    let subclass = SOURCE.replace(
        "class EmptyError(Exception): pass\nclass TooLongError(Exception): pass",
        "class TooLongError(Exception): pass\nclass EmptyError(TooLongError): pass",
    );
    assert_no_credit(&classify(
        &subclass,
        &test_source("TooLongError", "'abcdefghijklmnop'"),
        OLD,
        NEW,
    )?);
    for definition in [
        "class TooLongError(EmptyError): pass",
        "class TooLongError(Exception, Other): pass",
        "class TooLongError(Exception, metaclass=Custom): pass",
        "@decorate\nclass TooLongError(Exception): pass",
        "class TooLongError(Exception):\n    def __new__(cls): return EmptyError()",
    ] {
        let source = SOURCE.replace("class TooLongError(Exception): pass", definition);
        assert_no_credit(&classify(
            &source,
            &test_source("TooLongError", "'abcdefghijklmnop'"),
            OLD,
            NEW,
        )?);
    }
    Ok(())
}

#[test]
fn specific_exception_variant_requires_changed_raise_activation() -> Check {
    // Eight code points occupy sixteen UTF-8 bytes but never enter len(s) > 10.
    for argument in ["''", "'short'", "'éééééééé'", "input_value"] {
        assert_no_credit(&classify(
            SOURCE,
            &test_source("TooLongError", argument),
            OLD,
            NEW,
        )?);
    }
    for before in [
        "    raise TooLongError()\n",
        "    return 0\n",
        "    if s:\n        raise TooLongError()\n",
        "    if not s:\n        pass\n    else:\n        return 0\n",
        "    unknown()\n",
    ] {
        let source = SOURCE.replace("def parse(s):\n", &format!("def parse(s):\n{before}"));
        assert_no_credit(&classify(
            &source,
            &test_source("TooLongError", "'abcdefghijklmnop'"),
            OLD,
            NEW,
        )?);
    }
    for body in [
        "    parse('short')\n    with pytest.raises(TooLongError):\n        other()\n",
        "    with pytest.raises(TooLongError):\n        other()\n    parse('abcdefghijklmnop')\n",
        "    with pytest.raises(TooLongError):\n        other()\n        parse('abcdefghijklmnop')\n",
        "    if enabled:\n        with pytest.raises(TooLongError):\n            parse('abcdefghijklmnop')\n",
        "    return\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n",
        "    raise RuntimeError()\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n",
        "    assert False\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n",
        "    assert unknown_condition\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n",
        "    with pytest.raises(ValueError, match='x'):\n        pass\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n",
        "    with pytest.raises(ValueError, match='x'):\n        other()\n    with pytest.raises(TooLongError):\n        parse('abcdefghijklmnop')\n",
    ] {
        let tests =
            format!("{IMPORTS}\nfrom other_module import other\n\ndef test_variant():\n{body}");
        assert_no_credit(&classify(SOURCE, &tests, OLD, NEW)?);
    }
    Ok(())
}

#[test]
fn specific_exception_variant_requires_import_and_binding_identity() -> Check {
    let test = test_source("TooLongError", "'abcdefghijklmnop'");
    for imports in [
        "import pytest\nfrom fx.core import parse, EmptyError\nfrom foreign import TooLongError\n",
        "import pytest\nfrom foreign import parse\nfrom fx.core import EmptyError, TooLongError\n",
        "import pytest\nfrom fx import parse, EmptyError, TooLongError\n",
        "import pytest\nfrom fx.core import parse, EmptyError, TooLongError as Expected\n",
        "import fake as pytest\nfrom fx.core import parse, EmptyError, TooLongError\n",
    ] {
        let tests = test.replace(IMPORTS, imports);
        assert_no_credit(&classify(SOURCE, &tests, OLD, NEW)?);
    }
    let alias = test
        .replace("TooLongError\n", "TooLongError as Expected\n")
        .replace("pytest.raises(TooLongError)", "pytest.raises(Expected)");
    assert_no_credit(&classify(SOURCE, &alias, OLD, NEW)?);
    for binding in [
        "TooLongError = Exception\n",
        "pytest.raises = fake\n",
        "parse = fake\n",
        "TooLongError.__new__ = fake\n",
        "EmptyError.__bases__ = (TooLongError,)\n",
        "import builtins\nbuiltins.len = constant_len\n",
        "import builtins\nbuiltins.Exception = fake\n",
        "def rewrite():\n    global TooLongError\n    TooLongError = Exception\n",
    ] {
        let tests = test.replace(
            "\ndef test_variant",
            &format!("\n{binding}\ndef test_variant"),
        );
        assert_no_credit(&classify(SOURCE, &tests, OLD, NEW)?);
    }
    for binding in [
        "    TooLongError = Exception\n",
        "    parse = fake\n",
        "    pytest = fake\n",
    ] {
        let tests = test.replace(
            "def test_variant():\n",
            &format!("def test_variant():\n{binding}"),
        );
        assert_no_credit(&classify(SOURCE, &tests, OLD, NEW)?);
    }
    for source in [
        format!("Exception = RuntimeError\n{SOURCE}"),
        format!("__builtins__ = custom_builtins\n{SOURCE}"),
        format!("len = lambda value: 0\n{SOURCE}"),
        format!("{SOURCE}\nTooLongError = EmptyError\n"),
        format!(
            "{SOURCE}\ndef rewrite():\n    global TooLongError\n    TooLongError = EmptyError\n"
        ),
        SOURCE.replace("def parse(s):", "def parse(s, TooLongError):"),
        SOURCE.replace("    return len(s)", "    return len(s)\n    len = other"),
        SOURCE.replace("    return len(s)", "    yield len(s)"),
    ] {
        assert_no_credit(&classify(&source, &test, OLD, NEW)?);
    }
    for tests in [
        test.replace("def test_variant", "@pytest.mark.skip\ndef test_variant"),
        test.replace("def test_variant():", "def test_variant(fixture):"),
        format!("{test}    yield 1\n"),
    ] {
        assert_no_credit(&classify(SOURCE, &tests, OLD, NEW)?);
    }
    // A relation may follow an owner reexport; this identity proof must not.
    let file = Path::new("src/fx/core.py");
    let mut owners = extract_owners(file, SOURCE);
    for owner in owners.iter_mut().filter(|owner| owner.name == "parse") {
        owner.reexport_modules.push("fx".to_string());
    }
    let mixed_imports = test.replace(
        IMPORTS,
        "import pytest\nfrom fx import parse\nfrom fx.core import EmptyError, TooLongError\n",
    );
    let tests = extract_tests(Path::new("tests/test_core.py"), &mixed_imports);
    let finding = classify_change_with_context(
        file,
        8,
        NEW,
        Some(OLD),
        &owners,
        &tests,
        PythonNoBehaviorContext {
            sole_line_replacement: true,
            pytest_import_unshadowed: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(SOURCE),
    )
    .ok_or_else(|| "the reexport control needs its changed-raise finding".to_string())?;
    assert!(
        !finding.related_tests.is_empty(),
        "the reexport relation must exist without variant credit: {finding:?}"
    );
    assert_no_credit(&finding);
    Ok(())
}

#[test]
fn specific_exception_variant_requires_a_type_only_single_line_edit() -> Check {
    let test = test_source("TooLongError", "'abcdefghijklmnop'");
    for (old, new) in [
        (
            "        raise TooLongError('old')",
            "        raise TooLongError('new')",
        ),
        (
            "        raise EmptyError('old')",
            "        raise TooLongError('new')",
        ),
        (
            "        raise EmptyError(make())",
            "        raise TooLongError(make())",
        ),
        (
            "        raise EmptyError() from cause",
            "        raise TooLongError() from cause",
        ),
        ("        raise EmptyError", "        raise TooLongError"),
    ] {
        let source = SOURCE.replace(NEW, new);
        assert_no_credit(&classify(&source, &test, old, new)?);
    }
    assert_no_credit(&classify_with_context(SOURCE, &test, None, NEW, true)?);
    assert_no_credit(&classify_with_context(
        SOURCE,
        &test,
        Some(OLD),
        NEW,
        false,
    )?);
    assert_no_credit(&classify(
        SOURCE,
        &test,
        "\t\t\t\t\t\t\t\traise EmptyError()",
        NEW,
    )?);
    Ok(())
}

#[test]
fn specific_exception_variant_requires_verified_framework_import() -> Check {
    let file = Path::new("src/fx/core.py");
    let owners = extract_owners(file, SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_core.py"),
        &test_source("TooLongError", "'abcdefghijklmnop'"),
    );
    for pytest_import_unshadowed in [false, true] {
        let finding = classify_change_with_context(
            file,
            8,
            NEW,
            Some(OLD),
            &owners,
            &tests,
            PythonNoBehaviorContext {
                sole_line_replacement: true,
                pytest_import_unshadowed,
                ..PythonNoBehaviorContext::default()
            },
            Some(SOURCE),
        )
        .ok_or_else(|| "framework identity control needs its changed-raise finding".to_string())?;
        if pytest_import_unshadowed {
            assert_credited(&finding);
        } else {
            assert_no_credit(&finding);
        }
    }
    Ok(())
}

//! Tests for the weak `helper_owner_call` relation (#4568): a test that calls
//! a same-module caller of a changed top-level helper, when no test calls the
//! helper itself, is related with reach-only evidence and never earns an
//! oracle or `exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const VERSION_PY: &str = "def _validate_epoch(epoch):\n    if epoch >= 0:\n        return epoch\n    raise ValueError('bad')\n\n\nclass Version:\n    def __init__(self, text):\n        self.epoch = _validate_epoch(int(text))\n";
const VERSION_TEST_PY: &str = "from src.version import Version\n\n\ndef test_epoch_zero():\n    assert Version('0').epoch == 0\n";

fn finding_for(source: &str, tests_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/version.py"), source);
    let tests = extract_tests(Path::new("tests/test_version.py"), tests_source);
    classify_change(
        Path::new("src/version.py"),
        2,
        "    if epoch >= 0:",
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding for the changed helper line".to_string())
}

fn helper_relations(source: &str, tests_source: &str, helper: &str) -> Result<Vec<String>, String> {
    let owners = extract_owners(Path::new("src/version.py"), source);
    let tests = extract_tests(Path::new("tests/test_version.py"), tests_source);
    let owner = owners
        .iter()
        .find(|owner| owner.qualified_name == helper)
        .ok_or_else(|| format!("fixture must parse owner `{helper}`"))?;
    Ok(
        related_tests::helper_owner_call_candidates(owner, &owners, &tests)
            .iter()
            .map(|helper| {
                format!(
                    "{} via {} as {}",
                    helper.candidate.test.name,
                    helper.caller,
                    helper.candidate.relation.as_str()
                )
            })
            .collect(),
    )
}

#[test]
fn given_a_test_that_constructs_the_caller_class_when_the_private_helper_changes_then_it_is_weakly_exposed()
-> Result<(), String> {
    let finding = finding_for(VERSION_PY, VERSION_TEST_PY)?;

    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    let row = &finding.related_tests[0];
    assert_eq!(row.name, "test_epoch_zero");
    assert_eq!(
        row.oracle, None,
        "a helper_owner_call row carries no oracle"
    );
    assert_eq!(row.oracle_strength, OracleStrength::Unknown);
    for expected in [
        "related_test_relation: helper_owner_call (test_epoch_zero)",
        "related_test_uncertain: helper_owner_call (test_epoch_zero)",
        "helper_owner_call: Version.__init__ calls _validate_epoch (test_epoch_zero)",
    ] {
        assert!(
            finding.evidence.iter().any(|line| line == expected),
            "missing `{expected}` in {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn given_no_caller_test_when_the_private_helper_changes_then_it_stays_no_static_path()
-> Result<(), String> {
    let unrelated = "from src.version import Version\n\n\ndef test_other():\n    assert 1 == 1\n";
    let finding = finding_for(VERSION_PY, unrelated)?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert!(finding.related_tests.is_empty());
    Ok(())
}

#[test]
fn given_a_chain_of_top_level_callers_then_the_relation_follows_bounded_hops() -> Result<(), String>
{
    let source = "def leaf(x):\n    return x > 0\n\n\ndef mid(x):\n    return leaf(x)\n\n\ndef top(x):\n    return mid(x)\n";
    let tests = "from src.version import top\n\n\ndef test_top():\n    assert top(1)\n";
    assert_eq!(
        helper_relations(source, tests, "leaf")?,
        vec!["test_top via top as helper_owner_call".to_string()]
    );

    // Four hops exceed the bound of three.
    let deep = "def leaf(x):\n    return x > 0\n\n\ndef a(x):\n    return leaf(x)\n\n\ndef b(x):\n    return a(x)\n\n\ndef c(x):\n    return b(x)\n\n\ndef d(x):\n    return c(x)\n";
    let deep_tests = "from src.version import d\n\n\ndef test_d():\n    assert d(1)\n";
    assert!(helper_relations(deep, deep_tests, "leaf")?.is_empty());
    Ok(())
}

#[test]
fn given_a_test_that_calls_the_helper_directly_then_no_helper_relation_is_added()
-> Result<(), String> {
    let tests = "from src.version import Version, _validate_epoch\n\n\ndef test_direct():\n    assert _validate_epoch(0) == 0\n\n\ndef test_via_class():\n    assert Version('0').epoch == 0\n";
    let finding = finding_for(VERSION_PY, tests)?;

    let names: Vec<&str> = finding
        .related_tests
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, vec!["test_direct"], "{:?}", finding.evidence);
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("helper_owner_call")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_an_edge_that_is_not_a_bare_module_call_then_no_helper_relation_is_made()
-> Result<(), String> {
    let tests = "from src.version import make\n\n\ndef test_make():\n    assert make(1)\n";
    // (parameters of `make`, body of `make`)
    let cases = [
        ("x", "    return other._validate_epoch(x)\n"),
        (
            "x",
            "    # _validate_epoch(x)\n    return '_validate_epoch(x)'\n",
        ),
        ("x, _validate_epoch", "    return _validate_epoch(x)\n"),
        (
            "x",
            "    _validate_epoch = len\n    return _validate_epoch(x)\n",
        ),
    ];
    for (params, body) in cases {
        let source = format!(
            "def _validate_epoch(epoch):\n    return epoch >= 0\n\n\ndef make({params}):\n{body}"
        );
        assert!(
            helper_relations(&source, tests, "_validate_epoch")?.is_empty(),
            "no edge expected for body {body:?} with parameters {params:?}"
        );
    }
    // The unshadowed bare call is the control that proves the cases above
    // fail on the edge, not on the fixture.
    let control = "def _validate_epoch(epoch):\n    return epoch >= 0\n\n\ndef make(x):\n    return _validate_epoch(x)\n";
    assert_eq!(
        helper_relations(control, tests, "_validate_epoch")?,
        vec!["test_make via make as helper_owner_call".to_string()]
    );
    Ok(())
}

#[test]
fn given_a_decorated_caller_then_no_helper_relation_is_made() -> Result<(), String> {
    let tests = "from src.version import make\n\n\ndef test_make():\n    assert make(1)\n";
    let plain = "def helper(x):\n    return x > 0\n\n\ndef make(x):\n    return helper(x)\n";
    assert_eq!(
        helper_relations(plain, tests, "helper")?,
        vec!["test_make via make as helper_owner_call".to_string()]
    );
    let decorated = "import functools\n\n\ndef helper(x):\n    return x > 0\n\n\n@functools.lru_cache\ndef make(x):\n    return helper(x)\n";
    assert!(helper_relations(decorated, tests, "helper")?.is_empty());
    Ok(())
}

#[test]
fn given_unreadable_or_stacked_decorators_then_no_helper_relation_is_made() -> Result<(), String> {
    let tests = "from src.version import make\n\n\ndef test_make():\n    assert make(1)\n";
    for decorator in [
        "@registry[\"x\"]",
        "@(lambda f: f)",
        "@decos[0]",
        "@staticmethod\n@other",
        "@async_def",
        "@property(x)",
    ] {
        let source = format!(
            "def helper(x):\n    return x > 0\n\n\n{decorator}\ndef make(x):\n    return helper(x)\n"
        );
        assert!(
            helper_relations(&source, tests, "helper")?.is_empty(),
            "{decorator}"
        );
    }
    Ok(())
}

#[test]
fn given_binding_or_async_wrappers_then_a_helper_relation_is_still_made() -> Result<(), String> {
    let tests = "from src.version import make\n\n\ndef test_make():\n    assert make(1)\n";
    let source = "def helper(x):\n    return x > 0\n\n\n@staticmethod\n@classmethod\ndef make(x):\n    return helper(x)\n";
    assert_eq!(helper_relations(source, tests, "helper")?.len(), 1);
    for header in [
        "@property\ndef make(x):",
        "@classmethod\ndef make(x):",
        "async def make(x):",
    ] {
        let source =
            format!("def helper(x):\n    return x > 0\n\n\n{header}\n    return helper(x)\n");
        assert_eq!(
            helper_relations(&source, tests, "helper")?.len(),
            1,
            "{header}"
        );
    }
    Ok(())
}

#[test]
fn given_call_text_inside_a_docstring_then_contains_call_name_ignores_it() {
    assert!(!related_tests::contains_call_name(
        "\"\"\"\nhelper(1)\n\"\"\"\nx = 1\n",
        "helper"
    ));
    assert!(related_tests::contains_call_name(
        "x = helper(1)\n",
        "helper"
    ));
}

#[test]
fn given_a_duplicate_helper_definition_then_no_helper_relation_is_made() -> Result<(), String> {
    let source = "def helper(x):\n    return x > 0\n\n\ndef helper(x):\n    return x >= 0\n\n\ndef make(x):\n    return helper(x)\n";
    let tests = "from src.version import make\n\n\ndef test_make():\n    assert make(1)\n";
    assert!(helper_relations(source, tests, "helper")?.is_empty());
    Ok(())
}

#[test]
fn given_a_method_owner_then_no_helper_relation_is_made() -> Result<(), String> {
    let source = "class Box:\n    def check(self, x):\n        return x > 0\n\n\ndef make(x):\n    return Box().check(x)\n";
    let tests = "from src.version import make\n\n\ndef test_make():\n    assert make(1)\n";
    assert!(helper_relations(source, tests, "Box.check")?.is_empty());
    Ok(())
}

#[test]
fn given_a_skipped_caller_test_then_the_helper_relation_is_activation_limited() -> Result<(), String>
{
    let tests = "import pytest\nfrom src.version import Version\n\n\n@pytest.mark.skip(reason='x')\ndef test_epoch_zero():\n    assert Version('0').epoch == 0\n";
    assert_eq!(
        helper_relations(VERSION_PY, tests, "_validate_epoch")?,
        vec!["test_epoch_zero via Version.__init__ as test_activation_unestablished".to_string()]
    );
    Ok(())
}

#[test]
fn given_a_triple_quote_inside_a_single_line_string_or_comment_then_a_later_call_is_kept() {
    for prefix in [
        "text = '\"\"\"'\n",
        "text = \"'''\"\n",
        "# \"\"\" opens nothing\n",
        "text = 'it\\'s \"\"\"'\n",
    ] {
        let body = format!("{prefix}helper(1)\n");
        assert!(
            related_tests::contains_call_name(&body, "helper"),
            "{prefix}"
        );
    }
    let docstring = "\"\"\"\nmentions helper(1)\n\"\"\"\nx = 1\n";
    assert!(!related_tests::contains_call_name(docstring, "helper"));
    let after = "\"\"\"doc\"\"\"\nhelper(1)\n";
    assert!(related_tests::contains_call_name(after, "helper"));
}

#[test]
fn given_a_backslash_continued_string_then_a_call_on_the_continuation_line_is_hidden() {
    let continued = "x = \"about \\\nhelper(1)\"\n";
    assert!(!related_tests::contains_call_name(continued, "helper"));
    let after = "x = \"about \\\nmore\"\nhelper(1)\n";
    assert!(related_tests::contains_call_name(after, "helper"));
}

fn sees_call(body: &str) -> bool {
    related_tests::contains_call_name(body, "helper")
}

#[test]
fn given_a_multiline_f_string_replacement_field_then_a_call_inside_it_is_kept() {
    for body in [
        "assert f\"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        "assert f'''value: {\n    helper(1)\n}''' == x\n",
        "assert F\"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        "assert rf\"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        "assert fR\"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        "assert f\"\"\"a {x}\nb {\n    helper(1)\n}\"\"\" == x\n",
        "assert f\"\"\"{{literal}} {\n    helper(1)\n}\"\"\" == x\n",
        "assert f\"\"\"{\n    wrap(\n        helper(1)\n    )\n}\"\"\" == x\n",
        "assert f\"\"\"{x:{\n    helper(1)\n}}\"\"\" == x\n",
    ] {
        assert!(sees_call(body), "{body}");
    }
}

#[test]
fn given_text_outside_an_f_string_field_then_a_call_shaped_mention_stays_hidden() {
    for body in [
        // Not an f-string: braces are plain text.
        "assert \"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        "assert b\"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        // A space-separated `f` is not a prefix.
        "f = 1\nassert f + \"\"\"value: {\n    helper(1)\n}\"\"\" == x\n",
        // Text before, between and after the fields.
        "assert f\"\"\"value:\n    helper(1) is text\n{x}\"\"\" == x\n",
        "assert f\"\"\"{x}\n    helper(1) is text\n\"\"\" == x\n",
        "assert f\"\"\"{x} and\n    helper(1) is text {y}\"\"\" == x\n",
        // Doubled braces are literal text, not a field.
        "assert f\"\"\"a {{\n    helper(1)\n}}\"\"\" == x\n",
        // A nested string literal in a field is not a call.
        "assert f\"\"\"{\n    \"helper(1)\"\n}\"\"\" == x\n",
        // A field never closed before the string ends does not open code.
        "assert f\"\"\"{\n    helper(1)\n\"\"\" == x\n",
        // A docstring is still a docstring.
        "\"\"\"\nhelper(1)\n\"\"\"\n",
    ] {
        assert!(!sees_call(body), "{body}");
    }
    // The call after the f-string closes is visible again.
    assert!(sees_call("x = f\"\"\"{y}\n\"\"\"\nhelper(1)\n"));
    assert!(sees_call("x = f\"\"\"{\n    y\n}\"\"\"\nhelper(1)\n"));
}

#[test]
fn given_a_backslash_continued_string_containing_triple_quotes_then_later_calls_are_kept() {
    for body in [
        "x = 'begin \\\n\"\"\"'\nassert helper(1)\n",
        "x = \"begin \\\n'''\"\nassert helper(1)\n",
        "x = 'begin \\\r\n\"\"\"'\r\nassert helper(1)\r\n",
        "x = 'a \\\n b \\\n \"\"\"'\nhelper(1)\n",
    ] {
        assert!(sees_call(body), "{body:?}");
    }
    assert!(!sees_call("x = 'begin \\\n\"\"\"helper(1)'\n"));
    assert!(!sees_call("x = 'begin \\\r\n\"\"\"helper(1)'\r\n"));
}

#[test]
fn given_crlf_line_endings_then_a_continued_string_hides_its_continuation_line() {
    assert!(!sees_call("x = \"about \\\r\nhelper(1)\"\r\n"));
    assert!(!sees_call("x = \"about \\\r\nmore \\\r\nhelper(1)\"\r\n"));
    assert!(sees_call("x = \"about \\\r\nmore\"\r\nhelper(1)\r\n"));
    // Without a backslash the CRLF ends the string line.
    assert!(sees_call("x = \"about\"\r\nhelper(1)\r\n"));
}

#[test]
fn given_degenerate_string_text_then_the_call_lexer_never_panics() {
    let inputs = [
        "x = 'a\\",
        "x = 'a\\\r",
        "x = 'a\\\r\n",
        "x = \"\\",
        "\\",
        "x = '\\\u{e9}' helper(1)",
        "x = '\\\u{1f600}'\nhelper(1)\n",
        "x = 'abc",
        "x = 'abc\nhelper(1)\n",
        "x = \"\"\"abc",
        "x = f\"\"\"{",
        "x = f\"\"\"{\n    helper(1)",
        "x = f\"\"\"{{",
        "x = f\"\"\"{ 'a",
        "x = f\"\"\"{ \"\"\"",
        "x = f\"\"\"{\u{e9}\u{1f600}}\"\"\" helper(1)",
        "x = f'''{ '\\",
        "f\"\"\"",
        "\u{e9}f\"\"\"{ helper(1)",
        "\u{e9}\"\"\"{",
        "#",
        "",
    ];
    for input in inputs {
        let _ = related_tests::contains_call_name(input, "helper");
        for (idx, _) in input.char_indices().chain([(input.len(), ' ')]) {
            let _ = related_tests::python_text_hides_code(input, idx);
        }
    }
    // The values that are well-defined.
    assert!(!sees_call("x = 'a\\"));
    assert!(sees_call("helper(1); x = 'a\\"));
    assert!(sees_call("x = '\\\u{e9}' + helper(1)\n"));
    assert!(sees_call("x = 'abc\nhelper(1)\n"));
    assert!(!sees_call("x = 'abc helper(1)"));
}

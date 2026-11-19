//! A bare boolean assertion called with a pytest parametrize argname (#15310).

use super::*;

const COMMENT_OWNER: &str = "COMMENTCHARS = \"#;\"\n\n\ndef iscommentline(line: str) -> bool:\n    c = line.lstrip()[:1]\n    return c in COMMENTCHARS\n";

fn comment_tests(decorators: &str, signature: &str, body: &str) -> String {
    format!(
        "import pytest\nfrom src.rules import iscommentline\n\n\n{decorators}def test_comment({signature}):\n{body}"
    )
}

#[test]
fn given_a_parametrize_argname_then_bare_and_negated_asserts_are_exact_pins() {
    let tests = comment_tests(
        "@pytest.mark.parametrize(\"line\", [\"#qwe\", \" ;qwe\"])\n",
        "line",
        "    assert iscommentline(line)\n    assert not iscommentline(\"x\" + \"y\")\n",
    );
    let owners = extract_owners(Path::new(RULES), COMMENT_OWNER);
    let mut extracted = extract_tests(Path::new(TESTS_PATH), &tests);
    credit(&mut extracted, &owners, COMMENT_OWNER, &tests);

    let assertion = &extracted[0].assertions[0];
    assert_eq!(assertion.oracle_strength, OracleStrength::Strong);
    assert_eq!(assertion.gate_text, "assert iscommentline(line) == True");
    assert_eq!(assertion.text, "assert iscommentline(line)");
    // A computed argument is still no value the rows supply.
    assert_eq!(
        extracted[0].assertions[1].oracle_strength,
        OracleStrength::Smoke
    );
}

#[test]
fn given_two_parametrized_names_then_the_call_is_an_exact_pin() {
    let owner = "def within(value, limit) -> bool:\n    return value <= limit\n";
    let tests = "import pytest\nfrom src.rules import within\n\n\n@pytest.mark.parametrize(\"value, limit\", [(1, 2), (2, 2)])\ndef test_within(value, limit):\n    assert within(value, limit)\n";

    assert_eq!(
        credited_strengths(owner, tests),
        vec![OracleStrength::Strong]
    );
}

#[test]
fn given_a_name_no_parametrize_row_supplies_then_the_assertion_stays_smoke() {
    for (decorators, signature, body) in [
        // A fixture: its value is fixture code ripr does not run.
        ("", "line", "    assert iscommentline(line)\n"),
        // A local the test computes.
        (
            "",
            "",
            "    line = \"#qwe\"\n    assert iscommentline(line)\n",
        ),
        // The body rebinds the parametrized name before the assertion.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    line = line.upper()\n    assert iscommentline(line)\n",
        ),
        // A loop target shadows the parametrized name.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    for line in [\"x\"]:\n        assert iscommentline(line)\n",
        ),
        // One argument is parametrized, the other is a fixture.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line, other",
            "    assert iscommentline(line, other)\n",
        ),
        // `indirect=True` routes the row through a fixture of that name.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"], indirect=True)\n",
            "line",
            "    assert iscommentline(line)\n",
        ),
        // Argvalues named by a variable: no case is recorded.
        (
            "@pytest.mark.parametrize(\"line\", ROWS)\n",
            "line",
            "    assert iscommentline(line)\n",
        ),
        // Hypothesis generates the value; its `@example` row is not every input.
        (
            "@given(st.text())\n@example(\"#qwe\")\n",
            "line",
            "    assert iscommentline(line)\n",
        ),
        // A nested scope in the body is never enumerated.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    def inner():\n        return line\n    assert iscommentline(line)\n",
        ),
        // A walrus rebinds the name inside an expression.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    if (line := \"zzz\"):\n        pass\n    assert iscommentline(line)\n",
        ),
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    rows = [(line := row) for row in [\"zzz\"]]\n    assert iscommentline(line)\n",
        ),
        // A walrus target split from `:=` by a continuation or a comment.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    if (line \\\n            := \"zzz\"):\n        pass\n    assert iscommentline(line)\n",
        ),
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    if (line  # rebind\n            := \"zzz\"):\n        pass\n    assert iscommentline(line)\n",
        ),
        // Augmented assignment, `del` and a binding after the assertion.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    line += \"x\"\n    assert iscommentline(line)\n",
        ),
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    assert iscommentline(line)\n    del line\n",
        ),
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    assert iscommentline(line)\n    line = \"zzz\"\n",
        ),
        // A `match` capture.
        (
            "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
            "line",
            "    match \"zzz\":\n        case line:\n            pass\n    assert iscommentline(line)\n",
        ),
    ] {
        let tests = format!(
            "from hypothesis import example, given, strategies as st\nROWS = [\"#qwe\"]\n{}",
            comment_tests(decorators, signature, body)
        );
        let strengths = credited_strengths(COMMENT_OWNER, &tests);
        assert_eq!(strengths, vec![OracleStrength::Smoke], "{decorators}{body}");
    }
}

#[test]
fn given_an_async_parametrized_test_then_the_bare_assert_is_an_exact_pin() {
    let tests = comment_tests(
        "@pytest.mark.parametrize(\"line\", [\"#qwe\"])\n",
        "line",
        "    assert iscommentline(line)\n",
    )
    .replace("def test_comment", "async def test_comment");

    assert_eq!(
        credited_strengths(COMMENT_OWNER, &tests),
        vec![OracleStrength::Strong]
    );
}

#[test]
fn given_a_unittest_method_then_a_name_argument_stays_smoke() {
    let tests = "import unittest\nfrom src.rules import iscommentline\n\n\nclass CommentTest(unittest.TestCase):\n    def test_comment(self):\n        for line in [\"#qwe\"]:\n            with self.subTest(line=line):\n                self.assertTrue(iscommentline(line))\n";

    let strengths = credited_strengths(COMMENT_OWNER, tests);
    assert!(
        strengths
            .iter()
            .all(|strength| *strength != OracleStrength::Strong),
        "{strengths:?}"
    );
}

#[test]
fn given_an_operand_only_a_parametrized_bool_could_settle_then_the_assertion_stays_smoke() {
    // `return a and b` is a bool only when the call binds both to bool
    // literals; a parametrize name is not shown to be one.
    let owner = "def both(a, b):\n    return a and b\n";
    let tests = "import pytest\nfrom src.rules import both\n\n\n@pytest.mark.parametrize(\"a\", [True])\ndef test_both(a):\n    assert both(a, True)\n";

    assert_eq!(
        credited_strengths(owner, tests),
        vec![OracleStrength::Smoke]
    );
}

fn adult_finding(rows: &str) -> Result<Finding, String> {
    is_adult_finding(&format!(
        "import pytest\nfrom src.rules import is_adult\n\n\n@pytest.mark.parametrize(\"age\", [{rows}])\ndef test_adult(age):\n    assert is_adult(age)\n"
    ))
}

#[test]
fn given_parametrize_rows_on_the_boundary_then_the_changed_predicate_is_exposed()
-> Result<(), String> {
    assert_eq!(adult_finding("18, 30")?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn given_parametrize_rows_off_the_boundary_then_the_changed_predicate_is_not_exposed()
-> Result<(), String> {
    assert_ne!(adult_finding("30, 100")?.class, ExposureClass::Exposed);
    Ok(())
}

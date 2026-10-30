//! Tests for reading numpy.testing and assertpy equality helpers as the
//! equality they run (#10936, RIPR-SPEC-0238 rule 28).

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const ARRAYS: &str = "src/arrays.py";
const OWNER: &str = "def parse(text):\n    return [int(text) + 2]\n";

fn read_tests(tests_source: &str) -> Vec<PythonTest> {
    let mut tests = extract_tests(Path::new("tests/test_arrays.py"), tests_source);
    known_equality_helpers::credit_known_equality_helpers(&mut tests, |_| Some(tests_source));
    tests
}

/// The oracle strength of each assertion after the pass.
fn strengths(tests_source: &str) -> Vec<OracleStrength> {
    read_tests(tests_source)
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| assertion.oracle_strength.clone())
        .collect()
}

fn finding_class(tests_source: &str) -> Result<ExposureClass, String> {
    let owners = extract_owners(Path::new(ARRAYS), OWNER);
    let tests = read_tests(tests_source);
    classify_change(
        Path::new(ARRAYS),
        2,
        "    return [int(text) + 2]",
        &owners,
        &tests,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "expected a finding for the changed return".to_string())
}

fn numpy_test(imports: &str, body: &str) -> String {
    format!("{imports}\nfrom src.arrays import parse\n\n\ndef test_parse():\n{body}\n")
}

const NP: &str = "import numpy as np";
const CALL: &str = "    np.testing.assert_array_equal(parse(\"1\"), [2])";

#[test]
fn given_numpy_testing_through_its_alias_then_the_call_is_an_exact_pin() -> Result<(), String> {
    let source = numpy_test(NP, CALL);

    assert_eq!(strengths(&source), vec![OracleStrength::Strong]);
    assert_eq!(finding_class(&source)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn given_each_numpy_import_spelling_then_the_helper_is_credited() {
    for (imports, call) in [
        (
            "import numpy",
            "    numpy.testing.assert_array_equal(parse(\"1\"), [2])",
        ),
        (
            "import numpy.testing as npt",
            "    npt.assert_array_equal(parse(\"1\"), [2])",
        ),
        (
            "from numpy import testing",
            "    testing.assert_array_equal(parse(\"1\"), [2])",
        ),
        (
            "from numpy.testing import assert_array_equal",
            "    assert_array_equal(parse(\"1\"), [2])",
        ),
        (
            "from numpy.testing import assert_equal",
            "    assert_equal(parse(\"1\"), [2])",
        ),
        (
            NP,
            "    np.testing.assert_array_equal(parse(\"1\"), [2], err_msg=\"parse\", strict=True)",
        ),
    ] {
        assert_eq!(
            strengths(&numpy_test(imports, call)),
            vec![OracleStrength::Strong],
            "{imports}: {call}"
        );
    }
}

#[test]
fn given_the_expected_value_first_then_the_helper_is_still_an_exact_pin() {
    let source = numpy_test(NP, "    np.testing.assert_array_equal([2], parse(\"1\"))");

    assert_eq!(strengths(&source), vec![OracleStrength::Strong]);
}

#[test]
fn given_a_credited_helper_then_the_gate_reads_the_equivalent_comparison_and_text_stays() {
    let tests = read_tests(&numpy_test(NP, CALL));
    let assertion = &tests[0].assertions[0];

    assert_eq!(assertion.gate_text, "assert parse(\"1\") == [2]");
    assert_eq!(
        assertion.text,
        "np.testing.assert_array_equal(parse(\"1\"), [2])"
    );
}

#[test]
fn given_an_operand_that_regroups_under_equality_then_the_gate_parenthesizes_it() {
    let source = numpy_test(
        NP,
        "    np.testing.assert_array_equal(parse(\"1\") if True else [], not [2])",
    );
    let tests = read_tests(&source);

    assert_eq!(
        tests[0].assertions[0].gate_text,
        "assert (parse(\"1\") if True else []) == (not [2])"
    );
}

#[test]
fn given_a_file_that_only_sets_environment_through_monkeypatch_then_the_helper_is_still_credited() {
    let source = numpy_test(
        NP,
        "    monkeypatch.setenv(\"TZ\", \"UTC\")\n    np.testing.assert_array_equal(parse(\"1\"), [2])",
    );

    assert_eq!(strengths(&source), vec![OracleStrength::Strong]);
}

#[test]
fn given_assertpy_equality_then_the_chain_is_an_exact_pin() -> Result<(), String> {
    for (imports, call) in [
        (
            "from assertpy import assert_that",
            "    assert_that(parse(\"1\")).is_equal_to([2])",
        ),
        (
            "import assertpy",
            "    assertpy.assert_that(parse(\"1\")).is_equal_to([2])",
        ),
        (
            "import assertpy as ap",
            "    ap.assert_that(parse(\"1\")).is_equal_to([2])",
        ),
    ] {
        let source = numpy_test(imports, call);
        assert_eq!(strengths(&source), vec![OracleStrength::Strong], "{call}");
        assert_eq!(finding_class(&source)?, ExposureClass::Exposed, "{call}");
    }
    Ok(())
}

/// A call that must keep reading as an unclassified helper.
fn assert_not_credited(imports: &str, body: &str) {
    let source = numpy_test(imports, body);
    assert!(
        !strengths(&source).contains(&OracleStrength::Strong),
        "must stay an unclassified helper:\n{source}"
    );
}

#[test]
fn given_a_name_that_is_not_numpys_then_the_helper_is_not_credited() {
    // A project object named `np`, a project `testing` module, a missing
    // import, a relative import and a tests-local fluent helper.
    assert_not_credited("from mylib import np", CALL);
    assert_not_credited(
        "from mylib.testing import assert_array_equal",
        "    assert_array_equal(parse(\"1\"), [2])",
    );
    assert_not_credited("", CALL);
    assert_not_credited(
        "from .fluent import assert_that",
        "    assert_that(parse(\"1\")).is_equal_to([2])",
    );
    assert_not_credited(
        "from tests.fluent import assert_that",
        "    assert_that(parse(\"1\")).is_equal_to([2])",
    );
    assert_not_credited(
        "from assertion_helpers import assert_that",
        "    assert_that(parse(\"1\")).is_equal_to([2])",
    );
}

#[test]
fn given_an_import_inside_the_test_then_the_helper_is_not_credited() {
    assert_not_credited("", &format!("    import numpy as np\n{CALL}"));
}

#[test]
fn given_the_name_is_bound_twice_then_the_helper_is_not_credited() {
    assert_not_credited(&format!("{NP}\nimport other as np"), CALL);
    assert_not_credited(NP, &format!("    np = FakeNumpy()\n{CALL}"));
    assert_not_credited(NP, &format!("    for np in fakes:\n    {CALL}"));
    assert_not_credited(&format!("{NP}\n\ndef helper(np):\n    return np"), CALL);
}

#[test]
fn given_an_imported_callable_is_redefined_then_the_helper_is_not_credited() {
    let call = "    assert_array_equal(parse(\"1\"), [2])";
    let import = "from numpy.testing import assert_array_equal";
    assert_not_credited(
        import,
        &format!(
            "\n\ndef assert_array_equal(left, right):\n    pass\n\n\ndef test_inner():\n{call}"
        ),
    );
    assert_not_credited(
        &format!("{import}\n\n\ndef assert_array_equal(left, right):\n    pass"),
        call,
    );
}

#[test]
fn given_an_attribute_on_the_chain_is_rebound_then_the_helper_is_not_credited() {
    assert_not_credited(NP, &format!("    np.testing = FakeTesting()\n{CALL}"));
    assert_not_credited(
        NP,
        &format!("    np.testing.assert_array_equal = fake\n{CALL}"),
    );
}

#[test]
fn given_the_helper_is_patched_or_set_by_name_then_it_is_not_credited() {
    assert_not_credited(
        NP,
        &format!("    monkeypatch.setattr(np.testing, \"assert_array_equal\", fake)\n{CALL}"),
    );
    assert_not_credited(
        NP,
        &format!("    mocker.patch(\"numpy.testing.assert_array_equal\")\n{CALL}"),
    );
    assert_not_credited(
        "from assertpy import assert_that\nfrom unittest import mock",
        "    with mock.patch(\"assertpy.assert_that\"):\n        assert_that(parse(\"1\")).is_equal_to([2])",
    );
}

#[test]
fn given_a_patch_call_split_across_lines_then_the_helper_is_not_credited() {
    assert_not_credited(
        &format!("{NP}\nfrom unittest import mock"),
        &format!(
            "    p = mock.patch(\n        \"numpy.testing.assert_array_equal\",\n        lambda a, b: None,\n    )\n    p.start()\n{CALL}"
        ),
    );
    assert_not_credited(
        NP,
        &format!(
            "    monkeypatch.setattr(\n        \"numpy.testing.assert_array_equal\", lambda a, b: None\n    )\n{CALL}"
        ),
    );
}

#[test]
fn given_a_scalar_operand_then_the_numpy_helper_is_not_credited() {
    // `assert_array_equal(f(), 3)` passes for `[3]` too, so a shape change is unseen.
    for expected in [
        "2",
        "-2",
        "2.5",
        "1e3",
        "1j",
        "0x2",
        "0b1",
        "0o7",
        "0XFF",
        "True",
        "None",
        "\"x\"",
        "(2)",
        "b\"x\"",
        "r'x'",
        "f\"x\"",
        "u'x'",
        "rb'x'",
        "\"\"\"x\"\"\"",
        "'''x'''",
        "\"a\" \"b\"",
        "'a' 'it\\'s'",
        "\"\u{e9}\\\"\"",
    ] {
        assert_not_credited(
            NP,
            &format!("    np.testing.assert_array_equal(parse(\"1\"), {expected})"),
        );
        assert_not_credited(
            NP,
            &format!("    np.testing.assert_equal({expected}, parse(\"1\"))"),
        );
    }
}

#[test]
fn given_a_scalar_operand_then_assertpy_is_still_credited() {
    let source = "from assertpy import assert_that\nfrom src.arrays import parse\n\n\ndef test_parse():\n    assert_that(parse(\"1\")).is_equal_to(2)\n";
    assert_eq!(strengths(source), vec![OracleStrength::Strong]);
}

#[test]
fn given_the_file_rebinds_names_dynamically_then_the_helper_is_not_credited() {
    for marker in [
        "globals()[\"np\"] = fake",
        "exec(\"np = fake\")",
        "exec (\"np.testing.assert_array_equal = lambda *args: None\")",
        "exec\t(\"np = fake\")",
        "eval (\"np\")",
        "globals ()[\"np\"] = fake",
        "vars ()[\"np\"] = fake",
        "sys . modules[\"numpy\"] = fake",
        "from fake import *",
        "from fake import  *",
        "sys.modules[\"numpy\"] = fake",
        "np = importlib.import_module(\"fake\")",
        "np.testing.assert_array_equal.__code__ = (lambda *args: None).__code__",
    ] {
        assert_not_credited(NP, &format!("    {marker}\n{CALL}"));
    }
}

#[test]
fn given_the_name_is_stored_or_passed_along_then_the_helper_is_not_credited() {
    assert_not_credited(NP, &format!("    t = np.testing\n{CALL}"));
    assert_not_credited(NP, &format!("    use(np)\n{CALL}"));
    assert_not_credited(
        "from numpy.testing import assert_array_equal",
        "    check = assert_array_equal\n    assert_array_equal(parse(\"1\"), [2])",
    );
}

#[test]
fn given_a_tolerance_or_other_helper_then_it_is_not_credited() {
    for name in [
        "assert_allclose",
        "assert_almost_equal",
        "assert_array_almost_equal",
        "assert_array_less",
        "assert_string_equal",
        "assert_raises",
    ] {
        assert_not_credited(NP, &format!("    np.testing.{name}(parse(\"1\"), [2])"));
    }
}

#[test]
fn given_an_unrecognized_argument_shape_then_the_helper_is_not_credited() {
    for call in [
        "    np.testing.assert_array_equal(parse(\"1\"), [2], \"message\")",
        "    np.testing.assert_array_equal(parse(\"1\"), [2], decimal=2)",
        "    np.testing.assert_array_equal(*pair)",
        "    np.testing.assert_array_equal(parse(\"1\"))",
        "    np.testing.assert_array_equal(parse(\"1\"), [2], **options)",
    ] {
        assert_not_credited(NP, call);
    }
    for call in [
        "    assert_that(parse(\"1\"), \"description\").is_equal_to([2])",
        "    assert_that(parse(\"1\")).is_equal_to([2]).is_not_none()",
        "    assert_that(parse(\"1\")).described_as(\"x\").is_equal_to([2])",
        "    assert_that(parse(\"1\")).is_not_equal_to([2])",
        "    assert_that(parse(\"1\")).is_equal_to([2], \"extra\")",
    ] {
        assert_not_credited("from assertpy import assert_that", call);
    }
}

#[test]
fn given_the_helper_may_be_expected_to_fail_then_it_is_not_credited() {
    assert_not_credited(
        &format!("{NP}\nimport pytest"),
        "    with pytest.raises(AssertionError):\n        np.testing.assert_array_equal(parse(\"1\"), [3])",
    );
    assert_not_credited(
        NP,
        "    try:\n        np.testing.assert_array_equal(parse(\"1\"), [2])\n    except AssertionError:\n        pass",
    );
    assert_not_credited(
        "from assertpy import assert_that",
        "    with expect_failure():\n        assert_that(parse(\"1\")).is_equal_to([2])",
    );
}

#[test]
fn given_the_helper_in_a_loop_or_branch_then_it_is_still_credited() {
    let source = numpy_test(
        NP,
        "    for _ in range(2):\n        np.testing.assert_array_equal(parse(\"1\"), [2])",
    );

    assert_eq!(strengths(&source), vec![OracleStrength::Strong]);
}

#[test]
fn given_a_non_literal_operand_then_it_is_not_read_as_a_string_scalar() {
    // A name, call or concatenation expression is not a literal scalar, so the numpy helper still credits.
    for expected in ["name", "\"a\" + other", "bytes_of(\"x\")"] {
        let source = numpy_test(
            NP,
            &format!("    np.testing.assert_array_equal(parse(\"1\"), {expected})"),
        );
        assert_eq!(
            strengths(&source),
            vec![OracleStrength::Strong],
            "{expected}"
        );
    }
}

//! A mock call count is not the argument the mock received (#9684): the
//! count assertion reads `mock_expectation` / weak, whatever matcher spells
//! the comparison.

use super::*;

fn oracle_of(source: &str) -> Vec<(OracleKind, OracleStrength)> {
    extract_tests(Path::new("tests/send.test.ts"), source)
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| {
            (
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone(),
            )
        })
        .collect()
}

fn node_test(assertion: &str) -> String {
    format!(
        "import {{ test, mock }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ send }} from \"../src/send\";\n\ntest(\"send\", () => {{\n  const fn = mock.fn();\n  send(4, {{ send: fn }});\n  {assertion}\n}});\n"
    )
}

#[test]
fn a_node_mock_call_count_reads_weak_mock_expectation() {
    for assertion in [
        "assert.equal(fn.mock.callCount(), 1);",
        "assert.strictEqual(fn.mock.calls.length, 1);",
    ] {
        assert_eq!(
            oracle_of(&node_test(assertion)),
            vec![(OracleKind::MockExpectation, OracleStrength::Weak)],
            "{assertion}"
        );
    }
}

#[test]
fn a_mock_argument_pin_keeps_its_exact_reading() {
    let strong = oracle_of(&node_test(
        "assert.deepEqual(fn.mock.calls[0].arguments, [8]);",
    ));
    assert_eq!(strong.len(), 1);
    assert_ne!(strong[0].1, OracleStrength::Weak, "{strong:?}");
}

#[test]
fn a_call_count_on_a_non_mock_path_is_unchanged() {
    for assertion in [
        "assert.equal(counter.callCount(), 1);",
        "assert.equal(fn.mock.callCount() + 1, 2);",
    ] {
        let found = oracle_of(&node_test(assertion));
        assert_eq!(found.len(), 1, "{assertion}");
        assert_ne!(found[0].0, OracleKind::MockExpectation, "{assertion}");
    }
}

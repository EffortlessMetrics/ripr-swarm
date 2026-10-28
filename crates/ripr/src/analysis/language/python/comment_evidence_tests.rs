//! Comments are source context, never evidence for an assertion.
use super::owners_tests::extract_tests;
use super::source_utils::SourceText;
use crate::domain::OracleStrength;
use std::path::Path;

#[test]
fn comment_projection_preserves_strings_offsets_and_continuations() {
    let source = concat!(
        "value = ('# kept', r'\\# kept', \"escaped \\\"# kept\",\n",
        "         '''triple\n# kept\n''') # café removed\n",
        "other = 1 + \\\n    2 # removed\r\n",
    );
    let text = SourceText::new(source);
    let projected = text.semantic_text();
    assert_eq!(projected.len(), source.len());
    assert!(projected.contains("'# kept'"));
    assert!(projected.contains("r'\\# kept'"));
    assert!(projected.contains("\"escaped \\\"# kept\""));
    assert!(projected.contains("'''triple\n# kept\n'''"));
    assert!(projected.contains("1 + \\\n    2"));
    assert!(!projected.contains("removed"));
    assert!(!projected.contains("café"));
    assert_eq!(source.find("other"), projected.find("other"));
    assert_eq!(
        source.matches('\n').count(),
        projected.matches('\n').count()
    );
    assert!(projected.ends_with("\r\n"));
}

#[test]
fn comments_do_not_turn_tautologies_into_strong_oracles() {
    for statement in [
        "assert fee(10) == fee(10)",
        "assert fee(10) == fee(\n        10 # unrelated café\n    )",
        "self.assertEqual(fee(10), fee(\n        10 # unrelated\n    ))",
        "assert fee(fee(10)) == fee(\n        10 # idempotence is still weak\n    )",
    ] {
        let source = format!("def test_fee():\n    {statement}\n");
        let tests = extract_tests(Path::new("tests/test_fees.py"), &source);
        assert_eq!(tests.len(), 1, "{statement}");
        assert_eq!(tests[0].assertions.len(), 1, "{statement}");
        let assertion = &tests[0].assertions[0];
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Weak,
            "{statement}"
        );
        assert_eq!(assertion.text, statement);
        assert!(!assertion.gate_text.contains('#'));
    }
}

#[test]
fn real_hash_literals_and_strong_comparisons_keep_credit() {
    for statement in [
        "assert fee(10) == (\n        9 # unrelated\n    )",
        "assert fee(10) == '# expected'",
        "assert fee(10) == r'# expected'",
        "assert fee(10) == '''# expected'''",
        "self.assertEqual(fee(10), '# expected')",
    ] {
        let source = format!("def test_fee():\n    {statement}\n");
        let tests = extract_tests(Path::new("tests/test_fees.py"), &source);
        assert_eq!(tests.len(), 1, "{statement}");
        assert_eq!(tests[0].assertions.len(), 1, "{statement}");
        let assertion = &tests[0].assertions[0];
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Strong,
            "{statement}"
        );
        assert_eq!(assertion.text, statement);
        if statement.contains("# expected") {
            assert!(assertion.gate_text.contains("# expected"));
        }
    }
}

#[test]
fn comments_and_failure_messages_do_not_select_a_sibling_field() {
    use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
    use crate::domain::ProbeFamily;
    let focus = PythonAssertionFocus::for_change(
        ProbeFamily::FieldConstruction,
        "    self.total = amount",
        None,
    );
    for sibling in [
        "assert order.count == 1",
        "assert (order.count # total\n        == 1)",
        "assert order.count == 1, 'total'",
    ] {
        let source = format!("def test_order():\n    {sibling}\n    assert order.total > 0\n");
        let tests = extract_tests(Path::new("tests/test_order.py"), &source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].assertions.len(), 2);
        let selected = select_relevant_assertion(&tests[0].assertions, Some(&focus)).assertion();
        assert_eq!(
            selected.map(|assertion| assertion.text.as_str()),
            Some("assert order.total > 0")
        );
    }
}

#[test]
fn commented_exception_call_keeps_its_joined_message_comparison() -> Result<(), String> {
    use super::owners_tests::extract_owners;
    use crate::domain::ExposureClass;
    let file = Path::new("src/subject.py");
    let source = "def check(x):\n    if x < 0:\n        raise ValueError(\"negative value: new\")\n    return x\n";
    let owners = extract_owners(file, source);
    assert_eq!(
        owners.iter().filter(|owner| owner.name == "check").count(),
        1
    );
    for comment in ["", " # unrelated\n        "] {
        let tests_source = format!(
            "import pytest\nfrom src.subject import check\n\ndef test_negative():\n    with pytest.raises(ValueError,{comment} match=\"negative value\") as exc:\n        check(-1)\n    assert str(exc.value) == \"negative value: new\"\n"
        );
        let tests = extract_tests(Path::new("tests/test_subject.py"), &tests_source);
        assert_eq!(tests.len(), 1);
        let finding = super::classify::classify_change_with_source(
            file,
            3,
            "        raise ValueError(\"negative value: new\")",
            Some("        raise ValueError(\"negative value: old\")"),
            &owners,
            &tests,
            Some(source),
        )
        .ok_or("missing message-change finding")?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    }
    Ok(())
}

#[test]
fn internal_comments_do_not_change_whole_value_selection() {
    use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
    use crate::domain::ProbeFamily;
    let focus = PythonAssertionFocus::for_family(ProbeFamily::ReturnValue);
    for comment in ["", " # len(owner())\n       "] {
        let source =
            format!("def test_fee():\n    assert fee(1) == 2\n    assert (fee(2){comment} == 3)\n");
        let tests = extract_tests(Path::new("tests/test_fees.py"), &source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].assertions.len(), 2);
        let selected = select_relevant_assertion(&tests[0].assertions, Some(&focus)).assertion();
        assert_eq!(selected.map(|assertion| assertion.line), Some(3));
    }
}

//! Statement-order controls shared by instance and module effect observers.
use super::classify::classify_change_with_source;
use super::helper_assertions::apply_helper_assertions;
use super::owners_tests::extract_owners;
use super::related_tests::apply_class_facts;
use super::source_facts::extract_source_facts;
use crate::domain::{ExposureClass, Finding};
use std::{collections::BTreeMap, path::Path};

type Check = Result<(), String>;
const SOURCE: &str = "from types import SimpleNamespace\n\nresponse = SimpleNamespace(status_code=422)\n\nclass Recorder:\n    def __init__(self):\n        self.last = None\n    def note(self, kind):\n        if kind == \"error\":\n            self.last = \"fault\"\n\ndef checkout(expired=True):\n    response.status_code = 400\n";

fn finding(module: bool, body: &str, initial_error: bool, helper: &str) -> Result<Finding, String> {
    let source = if initial_error {
        SOURCE.replace("self.last = None", "self.last = \"error\"")
    } else {
        SOURCE.to_string()
    };
    let (before, after) = if module {
        (
            "    response.status_code = 422",
            "    response.status_code = 400",
        )
    } else {
        (
            "            self.last = \"error\"",
            "            self.last = \"fault\"",
        )
    };
    let file = Path::new("shop/route.py");
    let test_file = Path::new("tests/test_route.py");
    let test_source =
        format!("from shop.route import Recorder, checkout, response\n{helper}\n{body}\n");
    let mut owners = extract_owners(file, &source);
    apply_class_facts(&mut owners);
    let facts = extract_source_facts(test_file, &test_source);
    let mut tests = facts.tests;
    if tests.len() != 1 {
        return Err(format!("expected one test, got {}", tests.len()));
    }
    let modules = BTreeMap::from([(test_file.to_path_buf(), facts.helper_module)]);
    apply_helper_assertions(&mut tests, &modules);
    let line = source
        .lines()
        .position(|line| line == after)
        .ok_or("changed line absent")?
        + 1;
    classify_change_with_source(
        file,
        line,
        after,
        Some(before),
        &owners,
        &tests,
        Some(&source),
    )
    .ok_or_else(|| "expected exactly one changed-line finding".to_string())
}

#[test]
fn effect_order_rejects_same_line_reads_before_the_call() -> Check {
    for (module, body) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    assert rec.last == None; rec.note(\"error\")",
        ),
        (
            false,
            "def test_it(): rec = Recorder(); assert rec.last == None; rec.note(\"error\")",
        ),
        (
            false,
            "def test_it():\n    rec = Recorder()\n    marker = \"é;\"; assert rec.last == None; rec.note(\"error\")",
        ),
        (
            false,
            "def test_it():\n    rec = Recorder()\n    assert rec.last == None; rec.note(\n        \"error\"\n    )",
        ),
        (
            true,
            "def test_it():\n    assert response.status_code == 422; checkout()",
        ),
        (
            true,
            "def test_it(): assert response.status_code == 422; checkout()",
        ),
        (
            true,
            "def test_it():\n    marker = \"é;\"; assert response.status_code == 422; checkout()",
        ),
        (
            true,
            "def test_it():\n    assert response.status_code == 422; checkout(\n        True\n    )",
        ),
    ] {
        assert_ne!(
            finding(module, body, false, "")?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_preserves_same_line_and_multiline_reads_after_the_call() -> Check {
    for (module, body) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    rec.note(\"error\"); assert rec.last == \"error\"",
        ),
        (
            false,
            "def test_it():\n    rec = Recorder()\n    rec.note(\n        \"error\"\n    ); assert rec.last == \"error\"",
        ),
        (
            false,
            "def test_it():\n    rec = Recorder()\n    rec.note(\"error\"); assert (\n        rec.last == \"error\"\n    )",
        ),
        (
            true,
            "def test_it():\n    checkout(); assert response.status_code == 422",
        ),
        (
            true,
            "def test_it(): checkout(); assert response.status_code == 422",
        ),
        (
            true,
            "def test_it():\n    checkout(\n        True\n    ); assert response.status_code == 422",
        ),
        (
            true,
            "def test_it():\n    checkout(); assert (\n        response.status_code == 422\n    )",
        ),
    ] {
        assert_eq!(
            finding(module, body, false, "")?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_keeps_identical_assertions_at_distinct_columns() -> Check {
    for (module, body) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    assert rec.last == \"error\"; rec.note(\"error\"); assert rec.last == \"error\"",
        ),
        (
            true,
            "def test_it():\n    assert response.status_code == 422; checkout(); assert response.status_code == 422",
        ),
    ] {
        assert_eq!(
            finding(module, body, true, "")?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_does_not_borrow_guard_retention_from_a_pre_call_assertion() -> Check {
    let body = "def test_it():\n    rec = Recorder()\n    assert rec.last == \"error\"; rec.note(\"error\"); rec.last = \"error\"; assert rec.last == \"error\"";
    assert_ne!(
        finding(false, body, true, "")?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn effect_order_helper_calls_use_their_exact_callsite() -> Check {
    let helper = "\ndef assert_last(rec):\n    assert rec.last == \"error\"\n";
    let before = "def test_it():\n    rec = Recorder()\n    assert_last(rec); rec.note(\"error\")";
    assert_ne!(
        finding(false, before, true, helper)?.class,
        ExposureClass::Exposed
    );
    for body in [
        "def test_it():\n    rec = Recorder()\n    rec.note(\"error\"); assert_last(rec)",
        "def test_it():\n    rec = Recorder()\n    assert_last(rec); rec.note(\"error\"); assert_last(rec)",
    ] {
        assert_eq!(
            finding(false, body, true, helper)?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_preserves_an_inline_call_before_the_field_read() -> Check {
    for (module, body) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    assert rec.note(\"error\") is None and rec.last == \"error\"",
        ),
        (
            true,
            "def test_it():\n    assert checkout() is None and response.status_code == 422",
        ),
    ] {
        assert_eq!(
            finding(module, body, false, "")?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_keeps_unresolved_single_line_receiver_binding_weak() -> Check {
    // The existing receiver reader cannot bind this single-line setup.
    // Statement ordering does not broaden receiver identity.
    let body = "def test_it(): rec = Recorder(); rec.note(\"error\"); assert rec.last == \"error\"";
    assert_ne!(
        finding(false, body, false, "")?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn effect_order_does_not_borrow_reads_from_assertion_messages() -> Check {
    for (module, body) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    assert rec.last == None, (rec.note(\"error\"), rec.last)",
        ),
        (
            true,
            "def test_it():\n    assert response.status_code == 422, (checkout(), response.status_code)",
        ),
        (
            false,
            "import unittest\nclass TestOrder(unittest.TestCase):\n    def test_it(self):\n        rec = Recorder()\n        self.assertEqual(rec.last, None, msg=(rec.note(\"error\"), rec.last))",
        ),
        (
            true,
            "import unittest\nclass TestOrder(unittest.TestCase):\n    def test_it(self):\n        self.assertEqual(response.status_code, 422, msg=(checkout(), response.status_code))",
        ),
    ] {
        assert_ne!(
            finding(module, body, false, "")?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_keeps_message_bearing_post_call_reads() -> Check {
    for (module, body) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    rec.note(\"error\"); assert rec.last == \"error\", rec.last",
        ),
        (
            true,
            "def test_it():\n    checkout(); assert response.status_code == 422, response.status_code",
        ),
        (
            false,
            "import unittest\nclass TestOrder(unittest.TestCase):\n    def test_it(self):\n        rec = Recorder()\n        rec.note(\"error\"); self.assertEqual(rec.last, \"error\", msg=rec.last)",
        ),
        (
            true,
            "import unittest\nclass TestOrder(unittest.TestCase):\n    def test_it(self):\n        checkout(); self.assertEqual(response.status_code, 422, msg=response.status_code)",
        ),
    ] {
        assert_eq!(
            finding(module, body, false, "")?.class,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn effect_order_does_not_borrow_a_read_from_a_discarded_conjunct() -> Check {
    let body = "def test_it():\n    rec = Recorder()\n    assert rec.last == None and (rec.note(\"error\") is None or rec.last)";
    assert_ne!(
        finding(false, body, false, "")?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn effect_order_helper_capture_does_not_gain_effect_channel_credit() -> Check {
    let helper = "\ndef assert_equal(actual, expected):\n    assert actual == expected\n";
    let body = "def test_it():\n    rec = Recorder()\n    assert_equal(rec.last, rec.note(\"error\") or None)";
    let file = Path::new("shop/route.py");
    let mut owners = extract_owners(file, SOURCE);
    apply_class_facts(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.qualified_name == "Recorder.note")
        .ok_or("expected Recorder.note owner")?;
    let post_call = "def test_it():\n    rec = Recorder()\n    rec.note(\"error\"); assert_equal(rec.last, \"error\")";
    for (control, expected) in [(body, false), (post_call, true)] {
        let test_file = Path::new("tests/test_route.py");
        let test_source = format!("from shop.route import Recorder\n{helper}\n{control}\n");
        let facts = extract_source_facts(test_file, &test_source);
        let mut tests = facts.tests;
        apply_helper_assertions(
            &mut tests,
            &BTreeMap::from([(test_file.to_path_buf(), facts.helper_module)]),
        );
        let [test] = tests.as_slice() else {
            return Err("expected one native test".into());
        };
        let assertion = test
            .assertions
            .iter()
            .find(|assertion| assertion.helper_origin.is_helper())
            .ok_or("expected resolved helper assertion")?;
        assert!(assertion.source_range.is_some());
        let effect = super::effect_alignment::ChangedEffect::SelfField {
            attr: "last".to_string(),
        };
        let reads = super::effect_alignment::EffectReads::of(&effect, test, owner, true);
        assert_eq!(
            reads.observed_by(
                &assertion.gate_text,
                Some(assertion.line),
                assertion.source_range,
                &assertion.observed_operands,
                assertion.helper_origin.is_helper(),
            ),
            expected,
            "{control}"
        );
    }
    let captured = finding(false, body, false, helper)?;
    // A field read before the call compared with the implicit-None result
    // observes nothing of the write, through either credit route (#10178).
    assert_eq!(captured.class, ExposureClass::WeaklyExposed);
    assert_ne!(
        captured.alignment_reason.as_deref(),
        Some("strong_oracle_observes_owner_method_on_bound_receiver")
    );
    let after = "def test_it():\n    rec = Recorder()\n    assert_equal(rec.note(\"error\") or \"error\", rec.last)";
    // The field is captured as a helper argument, which the effect reader does
    // not treat as a field read; with the result branches closed this genuine
    // post-call comparison is a conservative miss, never a false credit.
    assert_eq!(
        finding(false, after, false, helper)?.class,
        ExposureClass::WeaklyExposed
    );

    // A blanket helper refusal would lose a real discriminator when the
    // owner returns the changed field after its write.
    let source = SOURCE
        .replace("self.last = None", "self.last = \"error\"")
        .replace(
            "            self.last = \"fault\"",
            "            self.last = \"fault\"\n        return self.last",
        );
    let file = Path::new("shop/route.py");
    let mut owners = extract_owners(file, &source);
    apply_class_facts(&mut owners);
    let test_file = Path::new("tests/test_route.py");
    let test_source = format!(
        "from shop.route import Recorder\n{helper}\ndef test_it():\n    rec = Recorder()\n    assert_equal(rec.last, rec.note(\"error\"))\n"
    );
    let facts = extract_source_facts(test_file, &test_source);
    let mut tests = facts.tests;
    apply_helper_assertions(
        &mut tests,
        &BTreeMap::from([(test_file.to_path_buf(), facts.helper_module)]),
    );
    let retained = classify_change_with_source(
        file,
        10,
        "            self.last = \"fault\"",
        Some("            self.last = \"error\""),
        &owners,
        &tests,
        Some(&source),
    )
    .ok_or("expected returned-field finding")?;
    assert_eq!(retained.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn effect_order_mock_inspection_follows_argument_evaluation() -> Check {
    let file = Path::new("shop/route.py");
    let source = "def notify(sender):\n    sender.send(\"new\")\n    return \"old\"\n";
    let owners = extract_owners(file, source);
    for (body, exposed) in [
        ("sender.send.assert_called_once_with(notify(sender))", true),
        (
            "sender.send(\"old\"); sender.send.assert_called_once_with(\"old\"); notify(sender)",
            false,
        ),
    ] {
        let test_source = format!(
            "from shop.route import notify\nfrom unittest.mock import Mock\n\ndef test_it():\n    sender = Mock()\n    {body}\n"
        );
        let facts = extract_source_facts(Path::new("tests/test_route.py"), &test_source);
        let found = classify_change_with_source(
            file,
            2,
            "    sender.send(\"new\")",
            Some("    sender.send(\"old\")"),
            &owners,
            &facts.tests,
            Some(source),
        )
        .ok_or("expected changed mock-call finding")?;
        assert_eq!(found.class == ExposureClass::Exposed, exposed, "{body}");
    }
    Ok(())
}

#[test]
fn implicit_none_owner_result_does_not_credit_a_field_write() -> Check {
    let helper = "\ndef assert_equal(actual, expected):\n    assert actual == expected\n";
    // Each negative names the owner call in a strong assertion but compares
    // only the implicit-None result, with the field read before the call or
    // not at all. The positive control reads the field after the call.
    for (module, body, exposed) in [
        (
            false,
            "def test_it():\n    rec = Recorder()\n    assert rec.note(\"error\") is None",
            false,
        ),
        (
            false,
            "def test_it():\n    rec = Recorder()\n    before = rec.last\n    result = rec.note(\"error\")\n    assert_equal(before, result)",
            false,
        ),
        (
            false,
            "def test_it():\n    rec = Recorder()\n    rec.note(\"error\")\n    assert rec.last == \"fault\"",
            true,
        ),
        (
            true,
            "def test_it():\n    assert_equal(response.status_code, checkout() or None)",
            false,
        ),
        (true, "def test_it():\n    assert checkout() is None", false),
        (
            true,
            "def test_it():\n    checkout()\n    assert response.status_code == 400",
            true,
        ),
    ] {
        let initial_error = !module;
        let found = finding(module, body, initial_error, helper)?;
        assert_eq!(found.class == ExposureClass::Exposed, exposed, "{body}");
    }
    Ok(())
}

#[test]
fn owner_that_returns_a_value_keeps_its_result_credit() -> Check {
    // A bare `return` or a returned value is a recognized value-return shape:
    // the closed no-value fact does not hold, so the call's result may still
    // observe the owner (a documented textual limit, not new credit).
    let source = SOURCE.replace(
        "            self.last = \"fault\"",
        "            self.last = \"fault\"\n        return self.last",
    );
    let file = Path::new("shop/route.py");
    let mut owners = extract_owners(file, &source);
    apply_class_facts(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.qualified_name == "Recorder.note")
        .ok_or("expected Recorder.note owner")?;
    assert!(!owner.term_facts.returns_no_value());
    let mut owners = extract_owners(file, SOURCE);
    apply_class_facts(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.qualified_name == "Recorder.note")
        .ok_or("expected Recorder.note owner")?;
    assert!(owner.term_facts.returns_no_value());
    Ok(())
}

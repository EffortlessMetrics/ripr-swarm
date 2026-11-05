//! The real Python diff producer must complete the existing packet route and
//! close the same module-constant gap when its suggested current pin is added.

use crate::analysis::{
    python_module_fstring_reader_finding, python_module_reader_assertion_parses,
    python_module_string_reader_finding, python_module_value_without_import_identity,
};
use crate::app::gap_bound_repair::{GapStartLookup, lookup_python_gap_start};
use crate::domain::{ExposureClass, Finding, FlowSinkKind, StageState};
use crate::output::agent_seam_packets::validate_agent_gap_record_packet;
use crate::output::python_repair_card::python_repair_card;
use std::fs;
use std::path::Path;

const SOURCE: &str = "GREETING = 'hi'\n\n\ndef greet(name):\n    return f'{GREETING}, {name}'\n";
const TEST: &str = "from fx.core import greet\n\n\ndef test_greet_mentions_name():\n    assert 'bob' in greet('bob')\n";
const SOURCE_DIFF: &str = "diff --git a/src/fx/core.py b/src/fx/core.py\nindex 1111111..2222222 100644\n--- a/src/fx/core.py\n+++ b/src/fx/core.py\n@@ -1,5 +1,5 @@\n-GREETING = 'hello'\n+GREETING = 'hi'\n \n \n def greet(name):\n     return f'{GREETING}, {name}'\n";
const TEST_DIFF: &str = "diff --git a/tests/test_t.py b/tests/test_t.py\nindex 3333333..4444444 100644\n--- a/tests/test_t.py\n+++ b/tests/test_t.py\n@@ -1,5 +1,6 @@\n from fx.core import greet\n \n \n def test_greet_mentions_name():\n     assert 'bob' in greet('bob')\n+    assert greet('bob') == 'hi, bob'\n";
const PYPROJECT: &str = "[project]\nname = \"fx\"\nversion = \"0.1.0\"\n\n[tool.pytest.ini_options]\npythonpath = [\"src\"]\n";
const CONFIG: &str = "[languages]\nenabled = [\"python\"]\n";

fn run_check(root: &Path) -> Result<crate::CheckOutput, String> {
    let config = crate::config::tests_only_parse(CONFIG)?;
    crate::app::check_workspace_with_config(
        crate::CheckInput {
            root: root.to_path_buf(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        },
        &config,
    )
}

fn source_finding(output: &crate::CheckOutput) -> Result<&Finding, String> {
    let mut findings = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with("src/fx/core.py"));
    match (findings.next(), findings.next()) {
        (Some(finding), None) => Ok(finding),
        _ => Err("the source edit must produce exactly one finding".to_string()),
    }
}

fn with_fixture(run: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-module-reader-{}-{stamp}",
        std::process::id()
    ));
    let proof = (|| -> Result<(), String> {
        fs::create_dir_all(root.join("src/fx"))
            .map_err(|error| format!("create source fixture: {error}"))?;
        fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test fixture: {error}"))?;
        for (file, source) in [
            ("src/fx/core.py", SOURCE),
            ("src/fx/__init__.py", ""),
            ("tests/test_t.py", TEST),
            ("pyproject.toml", PYPROJECT),
            ("ripr.toml", CONFIG),
            ("diff.patch", SOURCE_DIFF),
        ] {
            fs::write(root.join(file), source)
                .map_err(|error| format!("write fixture {file}: {error}"))?;
        }
        run(&root)
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove source fixture {}: {error}", root.display()));
    proof?;
    cleanup
}

#[test]
fn check_output_python_module_reader_starts_and_closes_the_same_gap() -> Result<(), String> {
    with_fixture(|root| {
        let before = run_check(root)?;
        let finding = source_finding(&before)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, StageState::Yes);
        assert_eq!(finding.ripr.propagate.state, StageState::Yes);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Weak);
        let reader_sinks = finding.flow_sinks.clone();
        assert_eq!(reader_sinks.len(), 1);
        assert_eq!(reader_sinks[0].kind, FlowSinkKind::ReturnValue);
        let card = python_repair_card(finding).ok_or("weak reader repair card missing")?;
        assert_eq!(card.missing_discriminator, "greet('bob') == \"hi, bob\"");
        assert_eq!(
            card.suggested_test_node_id.as_deref(),
            Some("tests/test_t.py::test_greet_mentions_name")
        );
        let gap = finding
            .canonical_gap
            .as_ref()
            .ok_or("weak reader gap identity missing")?
            .id
            .clone();
        let json = crate::render_check(&before, &crate::OutputFormat::Json)?;
        let start = lookup_python_gap_start(&before.findings, &json, &root.to_string_lossy(), &gap);
        let mut record = match start {
            GapStartLookup::Ready(record) => record,
            GapStartLookup::NotReady { reason } => return Err(reason),
            GapStartLookup::NoSuchGap => return Err("weak reader gap not found".to_string()),
        };
        validate_agent_gap_record_packet(&record)?;
        // Packet completeness remains the shared validator's decision.
        record.verification_commands.clear();
        assert!(validate_agent_gap_record_packet(&record).is_err());

        // Append the proposed exact current pin. The real adapter marks this
        // test edited in the diff; its preceding membership holds on both sides.
        fs::write(
            root.join("tests/test_t.py"),
            format!("{TEST}    assert greet('bob') == 'hi, bob'\n"),
        )
        .map_err(|error| format!("write exact current pin: {error}"))?;
        fs::write(root.join("diff.patch"), format!("{SOURCE_DIFF}{TEST_DIFF}"))
            .map_err(|error| format!("write source and test diff: {error}"))?;
        let after = run_check(root)?;
        let finding = source_finding(&after)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert_eq!(finding.ripr.propagate.state, StageState::Yes);
        assert_eq!(finding.flow_sinks, reader_sinks);
        assert_eq!(
            finding
                .canonical_gap
                .as_ref()
                .ok_or("exact reader gap identity missing")?
                .id,
            gap
        );
        assert!(python_repair_card(finding).is_none());
        let json = crate::render_check(&after, &crate::OutputFormat::Json)?;
        assert!(matches!(
            lookup_python_gap_start(&after.findings, &json, &root.to_string_lossy(), &gap),
            GapStartLookup::NotReady { .. }
        ));
        Ok(())
    })
}

fn assert_import_refusal(root: &Path) -> Result<(), String> {
    let output = run_check(root)?;
    let finding = source_finding(&output)?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert_eq!(finding.ripr.reach.state, StageState::Unknown);
    assert!(finding.flow_sinks.is_empty(), "{finding:?}");
    assert!(python_repair_card(finding).is_none());
    let gap = &finding
        .canonical_gap
        .as_ref()
        .ok_or("the refused import must retain its gap")?
        .id;
    let json = crate::render_check(&output, &crate::OutputFormat::Json)?;
    assert!(matches!(
        lookup_python_gap_start(&output.findings, &json, &root.to_string_lossy(), gap),
        GapStartLookup::NotReady { .. }
    ));
    Ok(())
}

#[test]
fn check_output_python_module_reader_refuses_source_shadows_and_recovers() -> Result<(), String> {
    with_fixture(|root| {
        for relative in ["core.py", "core/__init__.py"] {
            let shadow = root.join("tests/fx").join(relative);
            let parent = shadow.parent().ok_or("shadow needs a package directory")?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("create shadow package: {error}"))?;
            fs::write(root.join("tests/fx/__init__.py"), "")
                .map_err(|error| format!("write shadow package initializer: {error}"))?;
            fs::write(&shadow, "def greet(name):\n    return 'hi, bob'\n")
                .map_err(|error| format!("write unrelated reader: {error}"))?;
            for assertion in [
                "assert 'bob' in greet('bob')",
                "assert greet('bob') == 'hi, bob'",
                "assert greet('bob') == 'hello, bob'",
            ] {
                fs::write(
                    root.join("tests/test_t.py"),
                    TEST.replace("assert 'bob' in greet('bob')", assertion),
                )
                .map_err(|error| format!("write unchanged shadowed oracle: {error}"))?;
                fs::write(root.join("diff.patch"), SOURCE_DIFF)
                    .map_err(|error| format!("write production-only diff: {error}"))?;
                assert_import_refusal(root)?;
            }
            // An edited exact pin cannot repair an unproved import identity.
            fs::write(
                root.join("tests/test_t.py"),
                format!("{TEST}    assert greet('bob') == 'hi, bob'\n"),
            )
            .map_err(|error| format!("write edited shadowed oracle: {error}"))?;
            fs::write(root.join("diff.patch"), format!("{SOURCE_DIFF}{TEST_DIFF}"))
                .map_err(|error| format!("write shadowed test diff: {error}"))?;
            assert_import_refusal(root)?;

            // Removing only the competing package restores the scanned src
            // reader and its validator-complete repair, including cache reuse.
            fs::remove_dir_all(root.join("tests/fx"))
                .map_err(|error| format!("remove shadow package: {error}"))?;
            fs::write(root.join("tests/test_t.py"), TEST)
                .map_err(|error| format!("restore weak test: {error}"))?;
            fs::write(root.join("diff.patch"), SOURCE_DIFF)
                .map_err(|error| format!("restore production-only diff: {error}"))?;
            let output = run_check(root)?;
            let finding = source_finding(&output)?;
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
            assert_eq!(finding.ripr.reach.state, StageState::Yes);
            let gap = &finding
                .canonical_gap
                .as_ref()
                .ok_or("the restored reader needs its gap")?
                .id;
            let json = crate::render_check(&output, &crate::OutputFormat::Json)?;
            match lookup_python_gap_start(&output.findings, &json, &root.to_string_lossy(), gap) {
                GapStartLookup::Ready(record) => validate_agent_gap_record_packet(&record)?,
                GapStartLookup::NotReady { reason } => return Err(reason),
                GapStartLookup::NoSuchGap => return Err("restored gap not found".to_string()),
            }
        }
        Ok(())
    })
}

#[test]
fn check_output_python_module_reader_refuses_unproved_reexports() -> Result<(), String> {
    with_fixture(|root| {
        fs::create_dir_all(root.join("src/wrapper"))
            .map_err(|error| format!("create re-exporting package: {error}"))?;
        fs::write(
            root.join("src/wrapper/__init__.py"),
            "from fx.core import greet\n",
        )
        .map_err(|error| format!("write reader re-export: {error}"))?;
        fs::write(
            root.join("tests/wrapper.py"),
            "def greet(name):\n    return 'hi, bob'\n",
        )
        .map_err(|error| format!("write unrelated package shadow: {error}"))?;
        for (import, call) in [
            ("from wrapper import greet", "greet('bob')"),
            ("import wrapper", "wrapper.greet('bob')"),
        ] {
            for assertion in [
                format!("assert 'bob' in {call}"),
                format!("assert {call} == 'hi, bob'"),
                format!("assert {call} == 'hello, bob'"),
            ] {
                fs::write(
                    root.join("tests/test_t.py"),
                    format!("{import}\n\n\ndef test_greet_mentions_name():\n    {assertion}\n"),
                )
                .map_err(|error| format!("write re-exported oracle: {error}"))?;
                assert_import_refusal(root)?;
            }
        }
        Ok(())
    })
}

// Consumer assertions use analysis-owned test fixtures without reversing the layer boundary.
type Check = Result<(), String>;

const SETTINGS: &str = "LIMIT = 12\n\n\ndef over_limit(n):\n    return n > LIMIT\n";
const SETTINGS_OLD: &str = "LIMIT = 10";
const SETTINGS_NEW: &str = "LIMIT = 12";
const STRING_READER: &str =
    "GREETING = 'hi'\n\n\ndef greet(name):\n    return f'{GREETING}, {name}'\n";

fn limit_test(body: &str) -> String {
    format!("from hostile.settings import over_limit\n\n\ndef test_it():\n{body}\n")
}

fn fstring_test(assertion: &str) -> String {
    format!("from hostile.settings import greet\n\n\ndef test_it():\n    {assertion}\n")
}

fn assert_credited(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("related_test_relation: module_value_call")),
        "{:?}",
        finding.evidence
    );
}

fn assert_abstains(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("missing_import_graph")),
        "{:?}",
        finding.evidence
    );
}

fn assert_no_module_value_credit(finding: &Finding) {
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding.evidence.iter().any(|line| {
            line.starts_with("related_test_relation: module_value_call")
                || line.starts_with("module_value_read:")
        }),
        "{finding:?}"
    );
}

#[test]
fn module_string_reader_requires_positive_module_import_identity() -> Check {
    for edited in [false, true] {
        for assertion in [
            "assert 'bob' in greet('bob')",
            "assert greet('bob') == 'hi, bob'",
            "assert greet('bob') == 'hello, bob'",
        ] {
            let test = fstring_test(assertion);
            let finding = python_module_value_without_import_identity(
                STRING_READER,
                "GREETING = 'hi'",
                "GREETING = 'hello'",
                &[("tests/test_settings.py", &test)],
                edited,
            )
            .ok_or("an unresolved string import must retain a finding")?;
            assert_abstains(&finding);
            assert_eq!(finding.ripr.reach.state, crate::domain::StageState::Unknown);
            assert_no_module_value_credit(&finding);
            assert!(python_repair_card(&finding).is_none());
        }
    }
    // The additional import fact is specific to string evidence. Existing
    // numeric credit must not require it or gain a string-return sink.
    let test = limit_test("    assert over_limit(11)");
    let finding = python_module_value_without_import_identity(
        SETTINGS,
        SETTINGS_NEW,
        SETTINGS_OLD,
        &[("tests/test_settings.py", &test)],
        false,
    )
    .ok_or("the numeric edit must retain its finding")?;
    assert_credited(&finding);
    assert_eq!(
        finding.ripr.propagate.state,
        crate::domain::StageState::Weak
    );
    Ok(())
}

#[test]
fn module_string_reader_weak_membership_has_return_value_guidance() -> Check {
    let finding = python_module_fstring_reader_finding(
        "    return f'{GREETING}, {name}'",
        &fstring_test("assert 'bob' in greet('bob')"),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(finding.ripr.reach.state, crate::domain::StageState::Yes);
    assert_eq!(finding.ripr.propagate.state, crate::domain::StageState::Yes);
    assert_eq!(
        finding.ripr.reveal.observe.state,
        crate::domain::StageState::Weak
    );
    assert_eq!(
        finding.ripr.reveal.discriminate.state,
        crate::domain::StageState::Weak
    );
    assert!(finding.static_limit_kind.is_none(), "{finding:?}");
    let card =
        python_repair_card(&finding).ok_or("the bounded weak reader must supply a repair card")?;
    assert_eq!(card.missing_discriminator, "greet('bob') == \"hi, bob\"");
    assert!(card.recommended_test_shape.contains("return-value"));
    Ok(())
}

#[test]
fn module_string_reader_resolves_aliases_and_keeps_the_string_bound() -> Check {
    for (source, test, call) in [
        (
            STRING_READER.to_string(),
            "from hostile.settings import greet as welcome\n\ndef test_it():\n    assert 'bob' in welcome('bob')\n".to_string(),
            "welcome('bob')",
        ),
        (
            STRING_READER.replace("f'{GREETING}, {name}'", "GREETING + ', ' + name"),
            "from hostile import settings\n\ndef test_it():\n    assert 'bob' in settings.greet('bob')\n".to_string(),
            "settings.greet('bob')",
        ),
    ] {
        let finding = python_module_string_reader_finding(
            &source,
            "GREETING = 'hello'",
            &[("tests/test_settings.py", &test)],
            false,
            &[],
        )?;
        let card = python_repair_card(&finding)
            .ok_or("a resolved alias must keep its runnable reader")?;
        assert_eq!(card.missing_discriminator, format!("{call} == \"hi, bob\""));
    }
    for (argument, within_bound) in [
        (format!("{}xx", "☃".repeat(21_843)), true),
        (format!("{}xxx", "☃".repeat(21_843)), false),
    ] {
        for assertion in [
            format!("assert '☃' in greet('{argument}')"),
            format!("assert greet('{argument}') == 'hi{argument}'"),
        ] {
            let finding = python_module_fstring_reader_finding(
                "    return f'{GREETING}{name}'",
                &fstring_test(&assertion),
            )?;
            if within_bound {
                assert_ne!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
            } else {
                assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
                assert!(python_repair_card(&finding).is_none());
            }
        }
    }
    Ok(())
}

#[test]
fn module_string_reader_current_pin_closes_the_same_gap_after_a_test_edit() -> Check {
    let weak = python_module_string_reader_finding(
        STRING_READER,
        "GREETING = 'hello'",
        &[(
            "tests/test_settings.py",
            &fstring_test("assert 'bob' in greet('bob')"),
        )],
        false,
        &[],
    )?;
    for edited in [false, true] {
        let exact = python_module_string_reader_finding(
            STRING_READER,
            "GREETING = 'hello'",
            &[(
                "tests/test_settings.py",
                &fstring_test("assert greet('bob') == 'hi, bob'"),
            )],
            edited,
            &[],
        )?;
        assert_credited(&exact);
        assert_eq!(exact.ripr.propagate.state, crate::domain::StageState::Yes);
        assert_eq!(exact.flow_sinks, weak.flow_sinks);
        assert_eq!(
            weak.canonical_gap.as_ref().ok_or("weak gap identity")?.id,
            exact.canonical_gap.as_ref().ok_or("exact gap identity")?.id,
        );
        assert!(
            exact
                .evidence
                .iter()
                .any(|line| line.starts_with("module_value_current_pin:"))
        );
        assert!(python_repair_card(&exact).is_none());
    }
    for assertion in [
        "assert greet('bob') == 'hello, bob'",
        "assert 'bob' in greet('bob')",
        "assert 'hi' in greet('bob')",
    ] {
        let finding = python_module_string_reader_finding(
            STRING_READER,
            "GREETING = 'hello'",
            &[("tests/test_settings.py", &fstring_test(assertion))],
            true,
            &[],
        )?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{assertion}: {finding:?}"
        );
        assert!(python_repair_card(&finding).is_none());
    }
    Ok(())
}

#[test]
fn module_string_reader_current_pin_keeps_prefix_binding_and_activation_guards() -> Check {
    for prefix in ["assert 1 == 1\n    ", "assert 'bob' in greet('bob')\n    "] {
        let test = fstring_test(&format!("{prefix}assert 'hi, bob' == greet('bob')"));
        assert_credited(&python_module_string_reader_finding(
            STRING_READER,
            "GREETING = 'hello'",
            &[("tests/test_settings.py", &test)],
            true,
            &[],
        )?);
    }
    for test in [
        fstring_test("assert False\n    assert greet('bob') == 'hi, bob'"),
        fstring_test("assert False and greet('bob') == 'hi, bob'"),
        fstring_test("assert False and 1 == 1\n    assert greet('bob') == 'hi, bob'"),
        fstring_test(
            "assert False and 'bob' in greet('bob')\n    assert greet('bob') == 'hi, bob'",
        ),
        fstring_test("assert 'absent' in greet('bob')\n    assert greet('bob') == 'hi, bob'"),
        fstring_test("assert 'hi' in greet('bob')\n    assert greet('bob') == 'hi, bob'"),
        fstring_test("greet = lambda name: 'hi, bob'\n    assert greet('bob') == 'hi, bob'"),
        fstring_test("assert greet('bob') == 'hi, bob'")
            .replace("hostile.settings", "elsewhere.settings"),
        "from hostile.settings import greet\nimport pytest\n\n@pytest.mark.skip\ndef test_it():\n    assert greet('bob') == 'hi, bob'\n".to_string(),
        "from hostile.settings import greet\nimport pytest\n\n@pytest.mark.xfail\ndef test_it():\n    assert greet('bob') == 'hi, bob'\n".to_string(),
    ] {
        let finding = python_module_string_reader_finding(
            STRING_READER,
            "GREETING = 'hello'",
            &[("tests/test_settings.py", &test)],
            true,
            &[],
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{test}: {finding:?}");
        assert!(python_repair_card(&finding).is_none());
    }
    Ok(())
}

#[test]
fn module_string_reader_membership_requires_an_unchanged_true_oracle() -> Check {
    for (assertion, expected) in [
        ("assert 'bob' in greet('bob')", ExposureClass::WeaklyExposed),
        ("assert 'hello' in greet('bob')", ExposureClass::Exposed),
        ("assert 'hi' in greet('bob')", ExposureClass::Exposed),
        (
            "assert 'absent' in greet('bob')",
            ExposureClass::StaticUnknown,
        ),
        ("assert 'hello' not in greet('bob')", ExposureClass::Exposed),
    ] {
        let finding = python_module_fstring_reader_finding(
            "    return f'{GREETING}, {name}'",
            &fstring_test(assertion),
        )?;
        assert_eq!(finding.class, expected, "{assertion}: {finding:?}");
        if expected != ExposureClass::StaticUnknown {
            let row = finding
                .related_tests
                .first()
                .ok_or("reader test row missing")?;
            assert_eq!(row.oracle_kind, crate::domain::OracleKind::RelationalCheck);
            if expected == ExposureClass::Exposed {
                assert_eq!(row.oracle_strength, crate::domain::OracleStrength::Strong);
                assert_eq!(finding.ripr.propagate.state, crate::domain::StageState::Yes);
            }
        }
        if expected != ExposureClass::WeaklyExposed {
            assert!(python_repair_card(&finding).is_none());
        }
    }
    Ok(())
}

#[test]
fn module_string_reader_does_not_invent_reach_or_repair_from_call_spelling() -> Check {
    for assertion in [
        "assert False and 'bob' in greet('bob')",
        "assert True or 'bob' in greet('bob')",
        "assert 'bob' in (greet('bob') if False else 'bob')",
        "greet('bob')\n    assert 'bob' == 'bob'",
        "assert False\n    assert 'bob' in greet('bob')",
        "assert 'bob' in greet('bob')\n    assert 1 == 1",
        "greet = lambda name: name\n    assert 'bob' in greet('bob')",
    ] {
        let finding = python_module_fstring_reader_finding(
            "    return f'{GREETING}, {name}'",
            &fstring_test(assertion),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::StaticUnknown,
            "{assertion}: {finding:?}"
        );
        assert_eq!(finding.ripr.reach.state, crate::domain::StageState::Unknown);
        assert!(python_repair_card(&finding).is_none());
    }
    for body in [
        "    return f'{GREETING!s}, {name}'",
        "    return f'GREETING, {name}'",
        "    return name if name else GREETING",
        "    return unknown(GREETING, name)",
    ] {
        let finding = python_module_fstring_reader_finding(
            body,
            &fstring_test("assert 'bob' in greet('bob')"),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::StaticUnknown,
            "{body}: {finding:?}"
        );
        assert!(python_repair_card(&finding).is_none());
    }
    Ok(())
}

#[test]
fn module_string_reader_packet_stays_with_the_selected_test_and_escaped_value() -> Check {
    let tests = "from hostile.settings import greet\n\ndef test_aaa_unrelated():\n    greet('discarded')\n    assert 1 == 1\n\nclass TestA:\n    def test_same(self):\n        assert 'alice' in greet('alice')\n\nclass TestB:\n    def test_same(self):\n        assert 'bob' in greet('bob')\n";
    let source = STRING_READER.replace("'hi'", "'\\x07'");
    let finding = python_module_string_reader_finding(
        &source,
        "GREETING = 'hello'",
        &[("tests/test_settings.py", tests)],
        false,
        &[],
    )?;
    let card = python_repair_card(&finding).ok_or("the selected reader needs a complete card")?;
    assert_eq!(
        card.suggested_test_node_id.as_deref(),
        Some("tests/test_settings.py::TestA::test_same")
    );
    assert_eq!(
        card.missing_discriminator,
        "greet('alice') == \"\\u0007, alice\""
    );
    python_module_reader_assertion_parses(&format!("assert {}", card.missing_discriminator))?;
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line == "suggested_test_line: 8")
    );
    Ok(())
}

#[test]
fn module_string_reader_respects_foreign_writers_and_missing_import_identity() -> Check {
    for others in [
        vec![(
            "hostile/writer.py",
            "import hostile.settings as s\ns.GREETING = 'other'\n",
        )],
        vec![(
            "hostile/writer.py",
            "import hostile.settings as s\ns.greet.__code__ = replacement.__code__\n",
        )],
    ] {
        let finding = python_module_string_reader_finding(
            STRING_READER,
            "GREETING = 'hello'",
            &[(
                "tests/test_settings.py",
                &fstring_test("assert 'bob' in greet('bob')"),
            )],
            false,
            &others,
        )?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
        assert_eq!(finding.ripr.reach.state, crate::domain::StageState::Unknown);
        assert!(python_repair_card(&finding).is_none());
    }
    let test = fstring_test("assert 'bob' in greet('bob')")
        .replace("hostile.settings", "elsewhere.settings");
    let finding = python_module_string_reader_finding(
        STRING_READER,
        "GREETING = 'hello'",
        &[("tests/test_unrelated.py", &test)],
        false,
        &[],
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert_eq!(finding.ripr.reach.state, crate::domain::StageState::Unknown);
    assert_eq!(
        finding.ripr.reveal.discriminate.state,
        crate::domain::StageState::Unknown
    );
    Ok(())
}
